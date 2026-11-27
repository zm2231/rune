//! Pointer helpers for a window's events: where a mouse button acts, a pointer move, and the
//! OS drag-out session a move starts.

use winit::window::Window;

use super::WindowApp;
use crate::window::WakeEvent;

/// Starts an armed OS drag-out session from a pointer or touch move event.
pub(super) fn begin_host_drag_out(
    gpu: Option<&(std::sync::Arc<crate::render::Gpu>, crate::render::Renderer)>,
    l: &mut crate::window::surface::Win,
    rt: &crate::runtime::RuntimeCore,
    native_handle: &crate::window::handle::WindowHandle,
    scale: f32,
    start: crate::drag_out::DragOutStart,
) {
    let image = gpu.filter(|_| start.item.drag_image().is_none()).and_then(|(gpu, renderer)| {
        crate::drag_out::snapshot(gpu, renderer, (l.config.width, l.config.height), scale, rt.clear_color(), start.rect)
    });
    let (sink, path) = (rt.drag_end_sink(), start.path.clone());
    let handle = native_handle.clone();
    crate::drag_out::begin(&l.window, start, image, Box::new(move |outcome| {
        sink.borrow_mut().push((path, outcome));
        handle.send(WakeEvent::App);
    }));
    l.apply_cursor_core(rt, false);
}

/// Applies a pointer move in view points, returning whether the host must request a redraw and
/// any OS drag-out session the move armed. Leaves `timer_frame` quiet and `loop_frame` untouched
/// when the move changed nothing the next frame would run or paint.
pub(super) fn cursor_moved_core(
    app: &mut dyn WindowApp,
    timer_frame: &mut crate::window::surface::TimerFrame,
    loop_frame: &mut crate::window::surface::LoopFrame,
    x: f32,
    y: f32,
) -> (bool, Option<crate::drag_out::DragOutStart>) {
    let changed = app.observe_pointer(x, y);
    let drag_out = app.rt_mut().take_drag_out();
    let redraw = changed || drag_out.is_some();
    if redraw {
        timer_frame.no_longer_quiet();
        loop_frame.touched();
    }
    (redraw, drag_out)
}

/// Where a mouse button acts. On Windows, the point its message carries
/// (`platform::win32::press_point`). Elsewhere, where the runtime last saw the pointer, else
/// where the system says it is: a menu that took the mouse (`release_pointer_for_popup`) leaves
/// the runtime no pointer, and winit reports no move for a cursor that hasn't moved since, so a
/// click at the same spot after the menu closes would otherwise be lost. The pointer moves
/// there first, so hover is under it as the press lands.
pub(super) fn button_at(app: &mut dyn WindowApp, window: &Window) -> Option<(f32, f32)> {
    #[cfg(target_os = "windows")]
    if let Some((x, y)) = crate::platform::win32::press_point::current(window) {
        let input_scale = app.rt().input_scale();
        let at = (x as f32 / input_scale, y as f32 / input_scale);
        if app.rt().pointer() != Some(at) { app.pointer_move(at.0, at.1); }
        return Some(at);
    }
    if let Some(at) = app.rt().pointer() { return Some(at); }
    let (x, y) = crate::input::drop_pointer::DropPointer::new().sample(window, app.rt().zoom())?;
    app.pointer_move(x, y);
    Some((x, y))
}
