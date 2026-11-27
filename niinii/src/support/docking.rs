//! As of imgui 0.11, the official dockspace API isn't mature enough yet.
//! https://github.com/imgui-rs/imgui-rs/issues/683

use imgui::sys;

/// # Docking
pub trait UiDocking {
    #[doc(alias = "DockSpaceOverViewport")]
    fn dockspace_over_viewport(&self);
}

impl UiDocking for imgui::Ui {
    fn dockspace_over_viewport(&self) {
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
