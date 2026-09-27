//! The paint callback that draws a figure's list inside the window's render pass.
//!
//! [`GpuCallback`] is the one place the viewer meets `egui_wgpu`. The figure pane of [`crate::app`] adds it to egui's
//! painter in the figure's place among the interface's shapes, and when egui's renderer reaches it, it prepares and
//! draws the list through the [`GpuPainter`] kept in the renderer's callback resources, creating the painter on
//! first use. Everything it calls is in [`crate::gpu`], which has no egui in it, so that a host without egui draws
//! the same lists through the same painter from a render pass of its own.

use std::sync::Arc;

use crate::canvas::ScreenTransform;
use crate::gpu::{DrawList, GpuConfig, GpuPainter, Viewport};

impl Viewport {
    /// The viewport of an egui paint callback: the whole target, clipped to the painter's clip rectangle.
    #[must_use]
    pub fn from_callback(info: &egui::PaintCallbackInfo, to_screen: ScreenTransform) -> Self {
        Self {
            size_px: info.screen_size_px,
            pixels_per_point: info.pixels_per_point,
            to_screen,
            clip: info.clip_rect,
        }
    }
}

/// An egui paint callback that draws one list at one place through the [`GpuPainter`] kept in the renderer's
/// callback resources, creating the painter on first use.
pub struct GpuCallback {
    pub list: Arc<DrawList>,
    pub config: GpuConfig,
    pub to_screen: ScreenTransform,
}

impl egui_wgpu::CallbackTrait for GpuCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let painter = resources
            .entry::<GpuPainter>()
            .or_insert_with(GpuPainter::default);
        let viewport = Viewport::whole(
            screen.size_in_pixels,
            screen.pixels_per_point,
            self.to_screen,
        );
        painter.prepare(device, queue, self.config, &self.list, &viewport);
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(painter) = resources.get::<GpuPainter>() {
            let viewport = Viewport::from_callback(&info, self.to_screen);
            painter.paint(pass, &viewport, self.config, &self.list);
        }
    }
}
