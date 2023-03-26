//! https://github.com/imgui-rs/imgui-rs/issues/683

use std::os::raw::c_char;

use imgui::{sys, Direction};

pub struct DockNode {
    id: u32,
}

impl DockNode {
    fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn is_split(&self) -> bool {
        unsafe {
            let node = sys::igDockBuilderGetNode(self.id);
            if node.is_null() {
                false
            } else {
                sys::ImGuiDockNode_IsSplitNode(node)
            }
        }
    }
    /// Dock window into this dockspace
    #[doc(alias = "DockBuilder::DockWindow")]
    pub fn dock_window(&self, window: &str) {
        let window = imgui::ImString::from(window.to_string());
        unsafe { sys::igDockBuilderDockWindow(window.as_ptr(), self.id) }
    }

    #[doc(alias = "DockBuilder::SplitNode")]
    pub fn split<D, O>(&self, split_dir: Direction, size_ratio: f32, dir: D, opposite_dir: O)
    where
        D: FnOnce(DockNode),
        O: FnOnce(DockNode),
    {
        if self.is_split() {
            // Can't split an already split node (need to split the
            // node within)
            return;
        }

        let mut out_id_at_dir: sys::ImGuiID = 0;
        let mut out_id_at_opposite_dir: sys::ImGuiID = 0;
        unsafe {
            sys::igDockBuilderSplitNode(
                self.id,
                split_dir as i32,
                size_ratio,
                &mut out_id_at_dir,
                &mut out_id_at_opposite_dir,
            );
        }

        dir(DockNode::new(out_id_at_dir));
        opposite_dir(DockNode::new(out_id_at_opposite_dir));
    }
}

/// # Docking

pub struct UiDocking {}

impl UiDocking {
    #[doc(alias = "IsWindowDocked")]
    pub fn is_window_docked(&self) -> bool {
        unsafe { sys::igIsWindowDocked() }
    }
    /// Create dockspace with given label. Returns a handle to the
    /// dockspace which can be used to, say, programatically split or
    /// dock windows into it
    #[doc(alias = "DockSpace")]
    pub fn dockspace(&self, label: &str) -> DockNode {
        let label = imgui::ImString::from(label.to_string());
        unsafe {
            let id = sys::igGetID_Str(label.as_ptr() as *const c_char);
            sys::igDockSpace(
                id,
                [0.0, 0.0].into(),
                (sys::ImGuiDockNodeFlags_PassthruCentralNode) as i32,
                ::std::ptr::null::<sys::ImGuiWindowClass>(),
            );
            DockNode { id }
        }
    }

    #[doc(alias = "DockSpaceOverViewport")]
    pub fn dockspace_over_viewport(&self) {
        unsafe {
            sys::igPushStyleColor_Vec4(
                sys::ImGuiCol_WindowBg as i32,
                *sys::ImVec4_ImVec4_Float(0.0, 0.0, 0.0, 0.0),
            );
            sys::igPushStyleColor_Vec4(
                sys::ImGuiCol_Separator as i32,
                *sys::ImVec4_ImVec4_Float(0.0, 0.0, 0.0, 0.0),
            );
            sys::igDockSpaceOverViewport(
                sys::igGetMainViewport(),
                (sys::ImGuiDockNodeFlags_PassthruCentralNode) as i32,
                ::std::ptr::null::<sys::ImGuiWindowClass>(),
            );
            sys::igPopStyleColor(2);
        }
    }
}
