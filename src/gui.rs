use std::collections::HashMap;

use egui::{Context, Visuals, epaint};
use egui_wgpu::{RendererOptions, ScreenDescriptor};
use egui_wgpu::Renderer;

use egui_winit::State;
use egui_wgpu::wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};
use egui_wgpu::wgpu;
use egui_winit::winit::event::WindowEvent;
use egui_winit::winit::window::Window;

pub struct EguiRenderer {
    pub context: Context,
    state: State,
    renderer: Renderer,
    texure_ids: HashMap<String, epaint::TextureId>,

    // pub texture_atlas: egui::ImageSource<'static>,
}

impl EguiRenderer {
    pub fn new(
        device: &Device,
        output_color_format: TextureFormat,
        output_depth_format: Option<TextureFormat>,
        msaa_samples: u32,
        window: &Window,
    ) -> EguiRenderer {
        let egui_context = Context::default();
        let id = egui_context.viewport_id();

        const BORDER_RADIUS: f32 = 2.0;

        let visuals = Visuals {
            // window_rounding: egui::Rounding::same(BORDER_RADIUS),
            // window_shadow: Shadow::NONE,
            // menu_rounding: todo!(),
            ..Default::default()
        };

        egui_context.set_visuals(visuals);

        let egui_state = State::new(egui_context.clone(), id, &window, None, None, None);

        // egui_state.set_pixels_per_point(window.scale_factor() as f32);

        let renderer_options = RendererOptions {
            msaa_samples,
            depth_stencil_format: output_depth_format,
            dithering: false,
            predictable_texture_filtering: false,
        };

        let mut egui_renderer = Renderer::new(
            device,
            output_color_format,
            renderer_options,
            // output_depth_format,
            // msaa_samples,
        );

        egui_renderer.callback_resources.insert(BlockRenderResources {
            pipeline: None,
            texture_bind_group: None,
            camera_bind_group: None,
            block_meshes: Vec::new(),
        });

        // egui_extras::install_image_loaders(&egui_context);

        // let egui_texture_atlas = egui::include_image!("../res/texture_atlas.png");

        EguiRenderer {
            context: egui_context,
            state: egui_state,
            renderer: egui_renderer,
            texure_ids: HashMap::new(),
            // texture_atlas: egui_texture_atlas,
        }
    }

    pub fn handle_input(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(window, event);
        response.consumed
    }

    pub fn set_block_render_resources(
        &mut self,
        pipeline: wgpu::RenderPipeline,
        texture_bind_group: wgpu::BindGroup,
        camera_bind_group: wgpu::BindGroup,
        block_meshes: Vec<(wgpu::Buffer, wgpu::Buffer, u32)>,
    ) {
        self.renderer.callback_resources.insert(BlockRenderResources {
            pipeline: Some(pipeline),
            texture_bind_group: Some(texture_bind_group),
            camera_bind_group: Some(camera_bind_group),
            block_meshes,
        });
    }

    pub fn draw(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        window: &Window,
        window_surface_view: &TextureView,
        screen_descriptor: ScreenDescriptor,
        mut run_ui: impl FnMut(&Context),
    ) {
        // self.state.set_pixels_per_point(window.scale_factor() as f32);
        let raw_input = self.state.take_egui_input(&window);
        let full_output = self.context.run(raw_input, &mut run_ui);

        self.state
            .handle_platform_output(&window, full_output.platform_output);

        let tris = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(&device, &queue, *id, &image_delta);
        }
        self.renderer
            .update_buffers(&device, &queue, encoder, &tris, &screen_descriptor);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: window_surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                label: Some("egui main render pass"),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // SAFETY: This is safe because the render pass is dropped before the encoder
            // The lifetime issue is a bug in egui-wgpu 0.33 that was fixed in later versions
            let rpass_static = unsafe {
                std::mem::transmute::<_, &mut wgpu::RenderPass<'static>>(&mut rpass)
            };
            self.renderer.render(rpass_static, &tris, &screen_descriptor);
        }
        for x in &full_output.textures_delta.free {
            self.renderer.free_texture(x)
        }
    }

    pub fn register_wgpu_texture(
        &mut self,
        tex_str: String,
        device: &Device,
        texture: &TextureView,
        texture_filter: wgpu::FilterMode
    ) {
        let tex_id: epaint::TextureId = self.renderer.register_native_texture(device, texture, texture_filter);

        self.texure_ids.insert(tex_str, tex_id);
    }

    pub fn custom_painting(&mut self, ui: &mut egui::Ui) {
        let (rect, _response) =
            ui.allocate_exact_size(egui::Vec2::splat(300.0), egui::Sense::drag());

        // self.angle += response.drag_motion().x * 0.01;
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            CustomBlockCallback { block_type: 0 },
        ));
    }
}

pub struct CustomBlockCallback {
    pub block_type: u32,
}

impl egui_wgpu::CallbackTrait for CustomBlockCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &BlockRenderResources = resources.get().unwrap();
        resources.paint(render_pass, self.block_type);
    }
}

pub struct BlockRenderResources {
    pub pipeline: Option<wgpu::RenderPipeline>,
    pub texture_bind_group: Option<wgpu::BindGroup>,
    pub camera_bind_group: Option<wgpu::BindGroup>,
    pub block_meshes: Vec<(wgpu::Buffer, wgpu::Buffer, u32)>,
}

impl BlockRenderResources {
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>, block_type: u32) {
        if let (Some(pipeline), Some(texture_bind_group), Some(camera_bind_group)) = 
            (&self.pipeline, &self.texture_bind_group, &self.camera_bind_group) {
            if let Some((vertex_buffer, index_buffer, num_indices)) = self.block_meshes.get(block_type as usize) {
                if *num_indices > 0 {
                    render_pass.set_pipeline(pipeline);
                    render_pass.set_bind_group(0, texture_bind_group, &[]);
                    render_pass.set_bind_group(1, camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..*num_indices, 0, 0..1);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MultiplayerAction {
    None,
    Host { port: u16 },
    Join { url: String },
    Disconnect,
    RequestChunks,
}

pub struct MultiplayerPanel {
    pub join_address:  String,
    pub host_port:     String,
    pub status:        String,
    pub is_connected:  bool,
    pub is_hosting:    bool,
    pub lan_address:   Option<String>,
    pub chat_input:    String,
    pub chat_log:      Vec<(u32, String)>,
    pub my_id:         Option<u32>,
}

impl Default for MultiplayerPanel {
    fn default() -> Self {
        Self {
            join_address: "ws://127.0.0.1:7777".to_string(),
            host_port:    "7777".to_string(),
            status:       "Not connected".to_string(),
            is_connected: false,
            is_hosting:   false,
            lan_address:  None,
            chat_input:   String::new(),
            chat_log:     Vec::new(),
            my_id:        None,
        }
    }
}

impl MultiplayerPanel {
    pub fn draw(&mut self, ctx: &egui::Context) -> MultiplayerAction {
        let mut action = MultiplayerAction::None;

        egui::Window::new("Multiplayer")
            .default_pos([10.0, 200.0])
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let dot_color = if self.is_connected {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::RED
                    };
                    ui.colored_label(dot_color, "*");
                    ui.label(&self.status);
                });

                if let Some(ref addr) = self.lan_address {
                    ui.horizontal(|ui| {
                        ui.label("LAN address:");
                        ui.monospace(addr);
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(addr.clone());
                        }
                    });
                }

                ui.separator();

                if self.is_connected {
                    if ui.button("Disconnect").clicked() {
                        action = MultiplayerAction::Disconnect;
                    }

                    ui.separator();
                    ui.label("Chat:");
                    egui::ScrollArea::vertical()
                        .id_salt("chat_scroll")
                        .max_height(120.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for (sender_id, msg) in &self.chat_log {
                                ui.label(format!("[{}] {}", sender_id, msg));
                            }
                        });

                    ui.horizontal(|ui| {
                        let text_edit = ui.text_edit_singleline(&mut self.chat_input);
                        let send_pressed = ui.button("Send").clicked()
                            || (text_edit.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if send_pressed && !self.chat_input.is_empty() {
                            action = MultiplayerAction::RequestChunks;
                        }
                    });
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.collapsing("Host a LAN game", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Port:");
                                ui.text_edit_singleline(&mut self.host_port);
                            });
                            if ui.button("Start hosting").clicked() {
                                if let Ok(port) = self.host_port.parse::<u16>() {
                                    action = MultiplayerAction::Host { port };
                                } else {
                                    self.status = "Invalid port".to_string();
                                }
                            }
                        });

                        ui.separator();
                    }

                    ui.label("Join a game:");
                    ui.text_edit_singleline(&mut self.join_address);
                    if ui.button("Connect").clicked() {
                        action = MultiplayerAction::Join {
                            url: self.join_address.clone(),
                        };
                    }
                }
            });

        action
    }

    pub fn on_connected(&mut self, my_id: u32) {
        self.is_connected = true;
        self.my_id        = Some(my_id);
        self.status       = format!("Connected (ID {})", my_id);
    }

    pub fn push_chat(&mut self, sender_id: u32, message: String) {
        if self.chat_log.len() >= 200 {
            self.chat_log.remove(0);
        }
        self.chat_log.push((sender_id, message));
    }
}
