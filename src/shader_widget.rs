use eframe::egui::{self, Ui};
use eframe::egui_wgpu::CallbackTrait;
use eframe::{
    egui_wgpu::{self, wgpu},
    wgpu::util::DeviceExt,
};
use std::num::NonZeroU64;

pub struct TriangleRenderResources {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
}

impl TriangleRenderResources {
    pub fn prepare(&self, _device: &wgpu::Device, queue: &wgpu::Queue, angle: f32) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[angle, 0.0, 0.0, 0.0]),
        );
    }
}

pub struct TriangleCallback {
    pub size: f32,
}

impl CallbackTrait for TriangleCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        paint_callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &TriangleRenderResources = paint_callback_resources.get().unwrap();
        resources.prepare(device, queue, self.size);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        paint_callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &TriangleRenderResources = paint_callback_resources.get().unwrap();
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

pub fn custom_painting(size: f32, ui: &mut egui::Ui) {
    let (rect, _response) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::drag());

    // Clone locals so we can move them into the paint callback:
    // let angle: f32 = angle.clone();

    // The callback function for WGPU is in two stages: prepare, and paint.
    //
    // The prepare callback is called every frame before paint and is given access to the wgpu
    // Device and Queue, which can be used, for instance, to update buffers and uniforms before
    // rendering.
    //
    // You can use the main `CommandEncoder` that is passed-in, return an arbitrary number
    // of user-defined `CommandBuffer`s, or both.
    // The main command buffer, as well as all user-defined ones, will be submitted together
    // to the GPU in a single call.
    //
    // The paint callback is called after prepare and is given access to the render pass, which
    // can be used to issue draw commands.
    let callback = egui_wgpu::Callback::new_paint_callback(rect, TriangleCallback { size: size });

    ui.painter().add(callback);
}

pub fn init_shader<'a>(cc: &'a eframe::CreationContext<'a>) -> Option<()> {
    // Get the WGPU render state from the eframe creation context. This can also be retrieved
    // from `eframe::Frame` when you don't have a `CreationContext` available.
    let wgpu_render_state = cc.wgpu_render_state.as_ref()?;

    let device = &wgpu_render_state.device;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("custom3d"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/shaders.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("custom3d"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("custom3d"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        ..Default::default()
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("custom3d"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu_render_state.target_format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("custom3d"),
        contents: bytemuck::cast_slice(&[0.0_f32; 4]), // 16 bytes aligned!
        // Mapping at creation (as done by the create_buffer_init utility) doesn't require us to to add the MAP_WRITE usage
        // (this *happens* to workaround this bug )
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("custom3d"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // Because the graphics pipeline must have the same lifetime as the egui render pass,
    // instead of storing the pipeline in our `Custom3D` struct, we insert it into the
    // `paint_callback_resources` type map, which is stored alongside the render pass.
    wgpu_render_state
        .renderer
        .write()
        .callback_resources
        .insert(TriangleRenderResources {
            pipeline,
            bind_group,
            uniform_buffer,
        });
    Some(())
}

#[allow(dead_code)]
pub fn render_shader_widget(size: f32, ui: &mut Ui) {
    egui::Frame::canvas(ui.style()).show(ui, move |ui| {
        custom_painting(size, ui);
    });
}
