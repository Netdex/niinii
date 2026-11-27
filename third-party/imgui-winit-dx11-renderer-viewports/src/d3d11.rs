//! Device-level D3D11 objects shared by every viewport, and the draw-data
//! renderer. Adapted from imgui-dx11-renderer (MIT, Lukas Wirth).

use std::{mem, ptr, slice};

use imgui::{
    internal::RawWrapper, DrawCmd, DrawCmdParams, DrawData, DrawIdx, DrawVert, TextureId, Textures,
};
use winapi::{
    shared::{
        dxgi::{IDXGIAdapter, IDXGIDevice, IDXGIFactory},
        dxgiformat::{
            DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32_UINT,
            DXGI_FORMAT_R8G8B8A8_UNORM,
        },
        dxgitype::DXGI_SAMPLE_DESC,
        minwindef::{FALSE, TRUE},
        winerror::HRESULT,
    },
    um::{
        d3d11::*,
        d3dcommon::{D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D11_SRV_DIMENSION_TEXTURE2D},
    },
    Interface,
};
use wio::com::ComPtr;

use crate::RendererError;

/// Reserved texture id for the font atlas; never looked up in [`Textures`].
pub(crate) const FONT_TEX_ID: usize = !0;

const VERTEX_BUF_ADD_CAPACITY: usize = 5000;
const INDEX_BUF_ADD_CAPACITY: usize = 10000;

const VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vertex.dxbc"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pixel.dxbc"));

/// Fails only on negative HRESULTs: success codes such as `DXGI_STATUS_OCCLUDED`
/// are positive and must not be treated as errors.
pub(crate) fn check(hr: HRESULT, context: &'static str) -> Result<(), RendererError> {
    if hr < 0 {
        Err(RendererError::Hresult { context, hr })
    } else {
        Ok(())
    }
}

pub(crate) unsafe fn com_ptr<T: Interface>(
    context: &'static str,
    f: impl FnOnce(&mut *mut T) -> HRESULT,
) -> Result<ComPtr<T>, RendererError> {
    let mut raw = ptr::null_mut();
    check(f(&mut raw), context)?;
    if raw.is_null() {
        return Err(RendererError::Hresult { context, hr: 0 });
    }
    Ok(ComPtr::from_raw(raw))
}

#[repr(C)]
struct VertexConstantBuffer {
    mvp: [[f32; 4]; 4],
}

struct Buffer {
    buffer: ComPtr<ID3D11Buffer>,
    capacity: usize,
}

pub(crate) struct DeviceObjects {
    pub(crate) device: ComPtr<ID3D11Device>,
    pub(crate) context: ComPtr<ID3D11DeviceContext>,
    pub(crate) factory: ComPtr<IDXGIFactory>,
    vertex_shader: ComPtr<ID3D11VertexShader>,
    pixel_shader: ComPtr<ID3D11PixelShader>,
    input_layout: ComPtr<ID3D11InputLayout>,
    constant_buffer: ComPtr<ID3D11Buffer>,
    blend_state: ComPtr<ID3D11BlendState>,
    rasterizer_state: ComPtr<ID3D11RasterizerState>,
    depth_stencil_state: ComPtr<ID3D11DepthStencilState>,
    sampler: ComPtr<ID3D11SamplerState>,
    font_srv: ComPtr<ID3D11ShaderResourceView>,
    vertex_buffer: Option<Buffer>,
    index_buffer: Option<Buffer>,
    pub(crate) textures: Textures<ComPtr<ID3D11ShaderResourceView>>,
}

impl DeviceObjects {
    pub(crate) unsafe fn new(
        device: ComPtr<ID3D11Device>,
        fonts: &mut imgui::FontAtlas,
    ) -> Result<Self, RendererError> {
        let context = {
            let mut context = ptr::null_mut();
            device.GetImmediateContext(&mut context);
            ComPtr::from_raw(context)
        };
        let factory = acquire_factory(&device)?;

        let vertex_shader = com_ptr("CreateVertexShader", |out| {
            device.CreateVertexShader(
                VERTEX_SHADER.as_ptr().cast(),
                VERTEX_SHADER.len(),
                ptr::null_mut(),
                out,
            )
        })?;
        let pixel_shader = com_ptr("CreatePixelShader", |out| {
            device.CreatePixelShader(
                PIXEL_SHADER.as_ptr().cast(),
                PIXEL_SHADER.len(),
                ptr::null_mut(),
                out,
            )
        })?;

        let element = |name: &'static std::ffi::CStr, format, offset| D3D11_INPUT_ELEMENT_DESC {
            SemanticName: name.as_ptr(),
            SemanticIndex: 0,
            Format: format,
            InputSlot: 0,
            AlignedByteOffset: offset,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        };
        let layout = [
            element(c"POSITION", DXGI_FORMAT_R32G32_FLOAT, 0),
            element(c"TEXCOORD", DXGI_FORMAT_R32G32_FLOAT, 8),
            element(c"COLOR", DXGI_FORMAT_R8G8B8A8_UNORM, 16),
        ];
        let input_layout = com_ptr("CreateInputLayout", |out| {
            device.CreateInputLayout(
                layout.as_ptr(),
                layout.len() as u32,
                VERTEX_SHADER.as_ptr().cast(),
                VERTEX_SHADER.len(),
                out,
            )
        })?;

        let desc = D3D11_BUFFER_DESC {
            ByteWidth: mem::size_of::<VertexConstantBuffer>() as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let constant_buffer = com_ptr("CreateBuffer(constant)", |out| {
            device.CreateBuffer(&desc, ptr::null(), out)
        })?;

        let mut desc = D3D11_BLEND_DESC {
            AlphaToCoverageEnable: FALSE,
            IndependentBlendEnable: FALSE,
            RenderTarget: mem::zeroed(),
        };
        desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: TRUE,
            SrcBlend: D3D11_BLEND_SRC_ALPHA,
            DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D11_BLEND_OP_ADD,
            // Accumulate coverage into the destination alpha so transparent
            // windows composite correctly.
            SrcBlendAlpha: D3D11_BLEND_INV_DEST_ALPHA,
            DestBlendAlpha: D3D11_BLEND_ONE,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL as u8,
        };
        let blend_state = com_ptr("CreateBlendState", |out| {
            device.CreateBlendState(&desc, out)
        })?;

        let desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            FrontCounterClockwise: FALSE,
            DepthBias: 0,
            DepthBiasClamp: 0.0,
            SlopeScaledDepthBias: 0.0,
            DepthClipEnable: TRUE,
            ScissorEnable: TRUE,
            MultisampleEnable: FALSE,
            AntialiasedLineEnable: FALSE,
        };
        let rasterizer_state = com_ptr("CreateRasterizerState", |out| {
            device.CreateRasterizerState(&desc, out)
        })?;

        let stencil_op = D3D11_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D11_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
            StencilPassOp: D3D11_STENCIL_OP_KEEP,
            StencilFunc: D3D11_COMPARISON_ALWAYS,
        };
        let desc = D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: FALSE,
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
            DepthFunc: D3D11_COMPARISON_ALWAYS,
            StencilEnable: FALSE,
            StencilReadMask: 0,
            StencilWriteMask: 0,
            FrontFace: stencil_op,
            BackFace: stencil_op,
        };
        let depth_stencil_state = com_ptr("CreateDepthStencilState", |out| {
            device.CreateDepthStencilState(&desc, out)
        })?;

        let desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
            AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
            AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
            MipLODBias: 0.0,
            MaxAnisotropy: 0,
            ComparisonFunc: D3D11_COMPARISON_ALWAYS,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: 0.0,
        };
        let sampler = com_ptr("CreateSamplerState", |out| {
            device.CreateSamplerState(&desc, out)
        })?;

        let font_srv = create_font_texture(&device, fonts)?;

        Ok(Self {
            device,
            context,
            factory,
            vertex_shader,
            pixel_shader,
            input_layout,
            constant_buffer,
            blend_state,
            rasterizer_state,
            depth_stencil_state,
            sampler,
            font_srv,
            vertex_buffer: None,
            index_buffer: None,
            textures: Textures::new(),
        })
    }

    pub(crate) unsafe fn rebuild_font_texture(
        &mut self,
        fonts: &mut imgui::FontAtlas,
    ) -> Result<(), RendererError> {
        self.font_srv = create_font_texture(&self.device, fonts)?;
        Ok(())
    }

    /// Draws `draw_data` into the currently bound render target.
    /// `framebuffer_scale` maps imgui's logical coordinates to target pixels.
    pub(crate) unsafe fn render(
        &mut self,
        draw_data: &DrawData,
        framebuffer_scale: [f32; 2],
    ) -> Result<(), RendererError> {
        if draw_data.display_size[0] <= 0.0 || draw_data.display_size[1] <= 0.0 {
            return Ok(());
        }

        let vtx_count = draw_data.total_vtx_count as usize;
        let idx_count = draw_data.total_idx_count as usize;
        if self
            .vertex_buffer
            .as_ref()
            .is_none_or(|b| b.capacity < vtx_count)
        {
            self.vertex_buffer = Some(create_buffer::<DrawVert>(
                &self.device,
                D3D11_BIND_VERTEX_BUFFER,
                vtx_count + VERTEX_BUF_ADD_CAPACITY,
            )?);
        }
        if self
            .index_buffer
            .as_ref()
            .is_none_or(|b| b.capacity < idx_count)
        {
            self.index_buffer = Some(create_buffer::<DrawIdx>(
                &self.device,
                D3D11_BIND_INDEX_BUFFER,
                idx_count + INDEX_BUF_ADD_CAPACITY,
            )?);
        }

        self.write_buffers(draw_data)?;
        self.setup_render_state(draw_data, framebuffer_scale);

        let clip_off = draw_data.display_pos;
        let mut vertex_offset = 0;
        let mut index_offset = 0;
        let mut last_tex = None;
        for draw_list in draw_data.draw_lists() {
            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::Elements {
                        count,
                        cmd_params:
                            DrawCmdParams {
                                clip_rect,
                                texture_id,
                                vtx_offset,
                                idx_offset,
                            },
                    } => {
                        let r = D3D11_RECT {
                            left: ((clip_rect[0] - clip_off[0]) * framebuffer_scale[0]) as i32,
                            top: ((clip_rect[1] - clip_off[1]) * framebuffer_scale[1]) as i32,
                            right: ((clip_rect[2] - clip_off[0]) * framebuffer_scale[0]) as i32,
                            bottom: ((clip_rect[3] - clip_off[1]) * framebuffer_scale[1]) as i32,
                        };
                        if r.right <= r.left || r.bottom <= r.top {
                            continue;
                        }
                        if last_tex != Some(texture_id) {
                            let srv = self.texture(texture_id)?;
                            self.context.PSSetShaderResources(0, 1, &srv);
                            last_tex = Some(texture_id);
                        }
                        self.context.RSSetScissorRects(1, &r);
                        self.context.DrawIndexed(
                            count as u32,
                            (index_offset + idx_offset) as u32,
                            (vertex_offset + vtx_offset) as i32,
                        );
                    }
                    DrawCmd::ResetRenderState => {
                        self.setup_render_state(draw_data, framebuffer_scale);
                        last_tex = None;
                    }
                    DrawCmd::RawCallback { callback, raw_cmd } => {
                        callback(draw_list.raw(), raw_cmd)
                    }
                }
            }
            index_offset += draw_list.idx_buffer().len();
            vertex_offset += draw_list.vtx_buffer().len();
        }
        Ok(())
    }

    fn texture(&self, id: TextureId) -> Result<*mut ID3D11ShaderResourceView, RendererError> {
        if id.id() == FONT_TEX_ID {
            Ok(self.font_srv.as_raw())
        } else {
            self.textures
                .get(id)
                .map(|srv| srv.as_raw())
                .ok_or(RendererError::BadTexture(id))
        }
    }

    unsafe fn write_buffers(&self, draw_data: &DrawData) -> Result<(), RendererError> {
        let vb = self.vertex_buffer.as_ref().unwrap();
        let ib = self.index_buffer.as_ref().unwrap();

        let vtx = map_discard(&self.context, &vb.buffer, "Map(vertex)")?;
        let idx = match map_discard(&self.context, &ib.buffer, "Map(index)") {
            Ok(idx) => idx,
            Err(e) => {
                self.context.Unmap(vb.buffer.as_raw().cast(), 0);
                return Err(e);
            }
        };
        let mut vtx_dst = slice::from_raw_parts_mut(
            vtx.pData.cast::<DrawVert>(),
            draw_data.total_vtx_count as usize,
        );
        let mut idx_dst = slice::from_raw_parts_mut(
            idx.pData.cast::<DrawIdx>(),
            draw_data.total_idx_count as usize,
        );
        for draw_list in draw_data.draw_lists() {
            let (vbuf, ibuf) = (draw_list.vtx_buffer(), draw_list.idx_buffer());
            vtx_dst[..vbuf.len()].copy_from_slice(vbuf);
            idx_dst[..ibuf.len()].copy_from_slice(ibuf);
            vtx_dst = &mut vtx_dst[vbuf.len()..];
            idx_dst = &mut idx_dst[ibuf.len()..];
        }
        self.context.Unmap(vb.buffer.as_raw().cast(), 0);
        self.context.Unmap(ib.buffer.as_raw().cast(), 0);

        let cb = map_discard(&self.context, &self.constant_buffer, "Map(constant)")?;
        let l = draw_data.display_pos[0];
        let r = draw_data.display_pos[0] + draw_data.display_size[0];
        let t = draw_data.display_pos[1];
        let b = draw_data.display_pos[1] + draw_data.display_size[1];
        *cb.pData.cast::<VertexConstantBuffer>() = VertexConstantBuffer {
            mvp: [
                [2.0 / (r - l), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (t - b), 0.0, 0.0],
                [0.0, 0.0, 0.5, 0.0],
                [(r + l) / (l - r), (t + b) / (b - t), 0.5, 1.0],
            ],
        };
        self.context.Unmap(self.constant_buffer.as_raw().cast(), 0);
        Ok(())
    }

    unsafe fn setup_render_state(&self, draw_data: &DrawData, framebuffer_scale: [f32; 2]) {
        let ctx = &*self.context;
        let vp = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: draw_data.display_size[0] * framebuffer_scale[0],
            Height: draw_data.display_size[1] * framebuffer_scale[1],
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        ctx.RSSetViewports(1, &vp);

        let stride = mem::size_of::<DrawVert>() as u32;
        let offset = 0;
        let vb = self.vertex_buffer.as_ref().unwrap().buffer.as_raw();
        let ib = self.index_buffer.as_ref().unwrap().buffer.as_raw();
        let idx_format = if mem::size_of::<DrawIdx>() == 2 {
            DXGI_FORMAT_R16_UINT
        } else {
            DXGI_FORMAT_R32_UINT
        };
        ctx.IASetInputLayout(self.input_layout.as_raw());
        ctx.IASetVertexBuffers(0, 1, &vb, &stride, &offset);
        ctx.IASetIndexBuffer(ib, idx_format, 0);
        ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ctx.VSSetShader(self.vertex_shader.as_raw(), ptr::null(), 0);
        ctx.VSSetConstantBuffers(0, 1, &self.constant_buffer.as_raw());
        ctx.PSSetShader(self.pixel_shader.as_raw(), ptr::null(), 0);
        ctx.PSSetSamplers(0, 1, &self.sampler.as_raw());
        ctx.GSSetShader(ptr::null_mut(), ptr::null(), 0);
        ctx.HSSetShader(ptr::null_mut(), ptr::null(), 0);
        ctx.DSSetShader(ptr::null_mut(), ptr::null(), 0);
        ctx.CSSetShader(ptr::null_mut(), ptr::null(), 0);
        ctx.OMSetBlendState(self.blend_state.as_raw(), &[0.0; 4], 0xFFFF_FFFF);
        ctx.OMSetDepthStencilState(self.depth_stencil_state.as_raw(), 0);
        ctx.RSSetState(self.rasterizer_state.as_raw());
    }
}

unsafe fn acquire_factory(
    device: &ComPtr<ID3D11Device>,
) -> Result<ComPtr<IDXGIFactory>, RendererError> {
    let dxgi_device = device
        .cast::<IDXGIDevice>()
        .map_err(|hr| RendererError::Hresult {
            context: "QueryInterface(IDXGIDevice)",
            hr,
        })?;
    let adapter = com_ptr::<IDXGIAdapter>("IDXGIDevice::GetParent", |out| {
        dxgi_device.GetParent(&IDXGIAdapter::uuidof(), out as *mut *mut _ as *mut *mut _)
    })?;
    com_ptr::<IDXGIFactory>("IDXGIAdapter::GetParent", |out| {
        adapter.GetParent(&IDXGIFactory::uuidof(), out as *mut *mut _ as *mut *mut _)
    })
}

unsafe fn create_buffer<T>(
    device: &ComPtr<ID3D11Device>,
    bind: D3D11_BIND_FLAG,
    capacity: usize,
) -> Result<Buffer, RendererError> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: (capacity * mem::size_of::<T>()) as u32,
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: bind,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let buffer = com_ptr("CreateBuffer", |out| {
        device.CreateBuffer(&desc, ptr::null(), out)
    })?;
    Ok(Buffer { buffer, capacity })
}

unsafe fn map_discard(
    context: &ComPtr<ID3D11DeviceContext>,
    buffer: &ComPtr<ID3D11Buffer>,
    what: &'static str,
) -> Result<D3D11_MAPPED_SUBRESOURCE, RendererError> {
    let mut mapped = mem::zeroed();
    check(
        context.Map(
            buffer.as_raw().cast(),
            0,
            D3D11_MAP_WRITE_DISCARD,
            0,
            &mut mapped,
        ),
        what,
    )?;
    Ok(mapped)
}

unsafe fn create_font_texture(
    device: &ComPtr<ID3D11Device>,
    fonts: &mut imgui::FontAtlas,
) -> Result<ComPtr<ID3D11ShaderResourceView>, RendererError> {
    let tex = fonts.build_rgba32_texture();
    let desc = D3D11_TEXTURE2D_DESC {
        Width: tex.width,
        Height: tex.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: tex.data.as_ptr().cast(),
        SysMemPitch: tex.width * 4,
        SysMemSlicePitch: 0,
    };
    let texture = com_ptr::<ID3D11Texture2D>("CreateTexture2D(font)", |out| {
        device.CreateTexture2D(&desc, &data, out)
    })?;

    let mut srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
        u: mem::zeroed(),
    };
    *srv_desc.u.Texture2D_mut() = D3D11_TEX2D_SRV {
        MostDetailedMip: 0,
        MipLevels: 1,
    };
    let srv = com_ptr("CreateShaderResourceView(font)", |out| {
        device.CreateShaderResourceView(texture.as_raw().cast(), &srv_desc, out)
    })?;

    fonts.tex_id = TextureId::from(FONT_TEX_ID);
    Ok(srv)
}
