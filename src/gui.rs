use std::collections::HashMap;

use egui::{Context, Visuals, epaint};
use egui_wgpu::Renderer;
use egui_wgpu::{RendererOptions, ScreenDescriptor};

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};
use egui_winit::State;
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

        egui_renderer
            .callback_resources
            .insert(BlockRenderResources {
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
        self.renderer
            .callback_resources
            .insert(BlockRenderResources {
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
            let rpass_static =
                unsafe { std::mem::transmute::<_, &mut wgpu::RenderPass<'static>>(&mut rpass) };
            self.renderer
                .render(rpass_static, &tris, &screen_descriptor);
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
        texture_filter: wgpu::FilterMode,
    ) {
        let tex_id: epaint::TextureId =
            self.renderer
                .register_native_texture(device, texture, texture_filter);

        self.texure_ids.insert(tex_str, tex_id);
    }

    pub fn get_texture_id(&self, name: &str) -> Option<epaint::TextureId> {
        self.texure_ids.get(name).copied()
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
        if let (Some(pipeline), Some(texture_bind_group), Some(camera_bind_group)) = (
            &self.pipeline,
            &self.texture_bind_group,
            &self.camera_bind_group,
        ) {
            if let Some((vertex_buffer, index_buffer, num_indices)) =
                self.block_meshes.get(block_type as usize)
            {
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
    pub join_address: String,
    pub host_port: String,
    pub status: String,
    pub is_connected: bool,
    pub is_hosting: bool,
    pub lan_address: Option<String>,
    pub chat_input: String,
    pub chat_log: Vec<(u32, String)>,
    pub my_id: Option<u32>,
}

impl Default for MultiplayerPanel {
    fn default() -> Self {
        Self {
            join_address: "ws://127.0.0.1:7777".to_string(),
            host_port: "7777".to_string(),
            status: "Not connected".to_string(),
            is_connected: false,
            is_hosting: false,
            lan_address: None,
            chat_input: String::new(),
            chat_log: Vec::new(),
            my_id: None,
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
        self.my_id = Some(my_id);
        self.status = format!("Connected (ID {})", my_id);
    }

    pub fn push_chat(&mut self, sender_id: u32, message: String) {
        if self.chat_log.len() >= 200 {
            self.chat_log.remove(0);
        }
        self.chat_log.push((sender_id, message));
    }
}

pub struct GameStats {
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub selected_block: u32,
    pub chunks_loaded: usize,
    pub cursor_locked: bool,
}

pub fn draw_stats_window(ctx: &egui::Context, stats: &GameStats, render_distance: &mut i32) {
    egui::Window::new("Bassicraft")
        .default_pos([10.0, 10.0])
        .show(ctx, |ui| {
            ui.heading("Game Stats");
            ui.separator();
            ui.label(format!(
                "Position: {:.1}, {:.1}, {:.1}",
                stats.pos_x, stats.pos_y, stats.pos_z
            ));
            ui.label(format!(
                "Direction: {:.2}, {:.2}, {:.2}",
                stats.dir_x, stats.dir_y, stats.dir_z
            ));
            ui.label(format!("Selected block: {}", stats.selected_block));
            ui.separator();
            ui.label(format!("Chunks loaded: {}", stats.chunks_loaded));
            ui.horizontal(|ui| {
                ui.label("Render distance:");
                let max_rd: i32 = if cfg!(target_arch = "wasm32") { 6 } else { 16 };
                ui.add(egui::Slider::new(render_distance, 1..=max_rd).suffix(" chunks"));
            });
            ui.separator();
            ui.label("Controls:");
            ui.label("  WASD - Move");
            ui.label("  Space - Jump");
            ui.label("  Mouse - Look around");
            ui.label("  Left Click - Break block");
            ui.label("  Right Click - Place block");
            ui.label("  X - Spawn following mob");
            ui.label("  P - Toggle cursor lock");
            ui.label("  ESC - Exit");
            ui.separator();
            ui.label(format!(
                "Cursor: {}",
                if stats.cursor_locked {
                    "Locked"
                } else {
                    "Unlocked"
                }
            ));
        });
}

const INV_SCALE: f32 = 3.0;
const INV_BG_W: f32 = 176.0;
const INV_BG_H: f32 = 166.0;
const INV_TEX_W: f32 = 256.0;
const INV_TEX_H: f32 = 256.0;
const SLOT_SRC: f32 = 18.0;

fn slot_screen_rect(origin: egui::Pos2, src_x: f32, src_y: f32) -> egui::Rect {
    let s = INV_SCALE;
    egui::Rect::from_min_size(
        egui::pos2(origin.x + src_x * s, origin.y + src_y * s),
        egui::vec2(SLOT_SRC * s, SLOT_SRC * s),
    )
}

fn draw_item_in_slot(
    painter: &egui::Painter,
    ui: &egui::Ui,
    slot_rect: egui::Rect,
    item: crate::player::ItemStack,
) {
    if item.is_empty() {
        return;
    }
    let border = INV_SCALE;
    let inner = slot_rect.shrink(border);
    painter.add(egui_wgpu::Callback::new_paint_callback(
        inner,
        CustomBlockCallback {
            block_type: item.item_id.saturating_sub(1),
        },
    ));
    if item.count > 1 {
        let font = egui::FontId::proportional(INV_SCALE * 4.5);
        painter.text(
            inner.max - egui::vec2(1.0, 0.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("{}", item.count),
            font,
            egui::Color32::WHITE,
        );
    }
    let _ = ui;
}

fn apply_left_click(slot: &mut crate::player::ItemStack, held: &mut crate::player::ItemStack) {
    use crate::player::ItemStack;
    match (slot.is_empty(), held.is_empty()) {
        (_, true) => std::mem::swap(slot, held),
        (true, false) => std::mem::swap(slot, held),
        (false, false) => {
            if slot.item_id == held.item_id {
                let leftover = slot.merge(*held);
                *held = if leftover == 0 {
                    ItemStack::EMPTY
                } else {
                    ItemStack::new(held.item_id, leftover)
                };
            } else {
                std::mem::swap(slot, held);
            }
        }
    }
}

fn apply_right_click(slot: &mut crate::player::ItemStack, held: &mut crate::player::ItemStack) {
    use crate::player::ItemStack;
    if held.is_empty() {
        if !slot.is_empty() {
            let half = (slot.count + 1) / 2;
            *held = ItemStack::new(slot.item_id, half);
            slot.count -= half;
            if slot.count == 0 {
                *slot = ItemStack::EMPTY;
            }
        }
    } else if slot.is_empty() {
        *slot = ItemStack::new(held.item_id, 1);
        held.count -= 1;
        if held.count == 0 {
            *held = ItemStack::EMPTY;
        }
    } else if slot.item_id == held.item_id && slot.count < 64 {
        slot.count += 1;
        held.count -= 1;
        if held.count == 0 {
            *held = ItemStack::EMPTY;
        }
    } else {
        std::mem::swap(slot, held);
    }
}

pub fn draw_inventory_window(
    ctx: &egui::Context,
    inv_tex: Option<egui::TextureId>,
    inventory: &mut [crate::player::ItemStack; 36],
    craft_input: &mut [crate::player::ItemStack; 4],
    craft_output: &mut crate::player::ItemStack,
    held_item: &mut crate::player::ItemStack,
) {
    let scale = INV_SCALE;
    let bg_w = INV_BG_W * scale;
    let bg_h = INV_BG_H * scale;

    egui::Window::new("##inventory_window")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size(egui::vec2(bg_w, bg_h))
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(bg_w, bg_h),
                egui::Sense::hover(),
            );
            let painter = ui.painter();
            let origin = rect.min;

            if let Some(tex) = inv_tex {
                let uv = egui::Rect::from_min_max(
                    egui::pos2(0.0, 0.0),
                    egui::pos2(INV_BG_W / INV_TEX_W, INV_BG_H / INV_TEX_H),
                );
                painter.image(tex, rect, uv, egui::Color32::WHITE);
            } else {
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(198, 198, 198));
            }

            let mut hover_slots: Vec<egui::Rect> = Vec::new();

            macro_rules! interact_slot {
                ($item:expr, $src_x:expr, $src_y:expr, $id:expr) => {{
                    let sr = slot_screen_rect(origin, $src_x, $src_y);
                    let resp = ui.interact(sr, egui::Id::new($id), egui::Sense::click());
                    if resp.hovered() {
                        hover_slots.push(sr);
                    }
                    if resp.clicked() {
                        apply_left_click(&mut $item, held_item);
                    }
                    if resp.secondary_clicked() {
                        apply_right_click(&mut $item, held_item);
                    }
                    draw_item_in_slot(painter, ui, sr, $item);
                }};
            }

            for r in 0..2usize {
                for c in 0..2usize {
                    let idx = r * 2 + c;
                    let sx = 98.0 + c as f32 * 18.0;
                    let sy = 18.0 + r as f32 * 18.0;
                    interact_slot!(craft_input[idx], sx, sy, ("craft_in", idx));
                }
            }

            {
                let sr = slot_screen_rect(origin, 154.0, 28.0);
                draw_item_in_slot(painter, ui, sr, *craft_output);
                let resp = ui.interact(sr, egui::Id::new("craft_out"), egui::Sense::click());
                if resp.clicked() && !craft_output.is_empty() && held_item.is_empty() {
                    *held_item = *craft_output;
                    *craft_output = crate::player::ItemStack::EMPTY;
                    for ci in craft_input.iter_mut() {
                        if !ci.is_empty() {
                            ci.count = ci.count.saturating_sub(1);
                            if ci.count == 0 {
                                *ci = crate::player::ItemStack::EMPTY;
                            }
                        }
                    }
                }
            }

            for row in 0..3usize {
                for col in 0..9usize {
                    let inv_idx = 9 + row * 9 + col;
                    let sx = 8.0 + col as f32 * 18.0;
                    let sy = 84.0 + row as f32 * 18.0;
                    interact_slot!(inventory[inv_idx], sx, sy, ("main", inv_idx));
                }
            }

            for col in 0..9usize {
                let sx = 8.0 + col as f32 * 18.0;
                interact_slot!(inventory[col], sx, 142.0, ("hotbar", col));
            }

            for sr in hover_slots {
                painter.rect_filled(sr, 0.0, egui::Color32::from_white_alpha(80));
            }
        });

    if !held_item.is_empty() {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("held_item"),
        ));
        if let Some(pos) = ctx.pointer_hover_pos() {
            let size = SLOT_SRC * INV_SCALE;
            let rect = egui::Rect::from_center_size(pos, egui::vec2(size, size));
            painter.add(egui_wgpu::Callback::new_paint_callback(
                rect,
                CustomBlockCallback {
                    block_type: held_item.item_id.saturating_sub(1),
                },
            ));
            if held_item.count > 1 {
                painter.text(
                    rect.max - egui::vec2(2.0, 0.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("{}", held_item.count),
                    egui::FontId::proportional(INV_SCALE * 4.5),
                    egui::Color32::WHITE,
                );
            }
        }
    }
}

pub fn draw_hotbar(
    ctx: &egui::Context,
    inventory: &[crate::player::ItemStack; 36],
    selected_slot: usize,
    center: egui::Pos2,
    screen_height: f32,
) {
    for i in 0..crate::player::HOTBAR_SIZE {
        egui::Area::new(egui::Id::new(format!("inv_slot {}", i)))
            .fixed_pos(egui::pos2(
                center.x - 64.0 * (crate::player::HOTBAR_SIZE as f32 / 2.0) + 64.0 * i as f32,
                screen_height - 60.0,
            ))
            .show(ctx, |ui| {
                let is_selected = selected_slot == i;
                let mut frame = egui::Frame::canvas(ui.style());
                if is_selected {
                    frame = frame.stroke(egui::Stroke::new(3.0, egui::Color32::WHITE));
                }
                frame.show(ui, |ui| {
                    let (rect, _response) =
                        ui.allocate_exact_size(egui::Vec2::splat(55.0), egui::Sense::empty());
                    let item = inventory[i];
                    if !item.is_empty() {
                        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                            rect,
                            CustomBlockCallback {
                                block_type: item.item_id.saturating_sub(1),
                            },
                        ));
                        if item.count > 1 {
                            ui.painter().text(
                                rect.max - egui::vec2(2.0, 0.0),
                                egui::Align2::RIGHT_BOTTOM,
                                format!("{}", item.count),
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                });
            });
    }
}

#[derive(Clone)]
pub struct AppOptions {
    pub render_distance: i32,
    pub mouse_sensitivity: f32,
    pub fov: f32,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            render_distance: 8,
            mouse_sensitivity: 0.4,
            fov: 45.0,
        }
    }
}

pub enum TitleAction {
    None,
    Play,
    Options,
    Quit,
}

pub fn draw_title_screen(ctx: &egui::Context) -> TitleAction {
    let mut action = TitleAction::None;

    let screen = ctx.content_rect();
    let center_x = screen.center().x;
    let total_h = 300.0_f32;
    let top_y = screen.center().y - total_h / 2.0;

    egui::Area::new(egui::Id::new("title_screen"))
        .fixed_pos(egui::pos2(center_x - 200.0, top_y))
        .show(ctx, |ui| {
            ui.set_width(400.0);

            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("BASSICRAFT 2")
                        .size(52.0)
                        .strong()
                        .color(egui::Color32::GRAY),
                );
                ui.label(
                    egui::RichText::new("test")
                        .size(18.0)
                        .color(egui::Color32::YELLOW),
                );

                ui.add_space(40.0);

                let btn_size = egui::vec2(200.0, 45.0);

                if ui
                    .add_sized(
                        btn_size,
                        egui::Button::new(egui::RichText::new("Play").size(20.0)),
                    )
                    .clicked()
                {
                    action = TitleAction::Play;
                }

                ui.add_space(10.0);

                if ui
                    .add_sized(
                        btn_size,
                        egui::Button::new(egui::RichText::new("Options").size(20.0)),
                    )
                    .clicked()
                {
                    action = TitleAction::Options;
                }

                ui.add_space(10.0);

                #[cfg(not(target_arch = "wasm32"))]
                if ui
                    .add_sized(
                        btn_size,
                        egui::Button::new(egui::RichText::new("Quit").size(20.0)),
                    )
                    .clicked()
                {
                    action = TitleAction::Quit;
                }
            });
        });

    action
}

pub fn draw_options_screen(ctx: &egui::Context, options: &mut AppOptions) -> bool {
    let mut back = false;

    let screen = ctx.content_rect();
    let center_x = screen.center().x;
    let top_y = screen.center().y - 200.0;

    egui::Area::new(egui::Id::new("options_screen"))
        .fixed_pos(egui::pos2(center_x - 200.0, top_y))
        .show(ctx, |ui| {
            ui.set_width(400.0);

            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Options")
                        .size(36.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.add_space(30.0);

                egui::Grid::new("options_grid")
                    .num_columns(2)
                    .spacing([20.0, 14.0])
                    .show(ui, |ui| {
                        let max_rd: i32 = if cfg!(target_arch = "wasm32") { 6 } else { 16 };

                        ui.label("Render distance:");
                        ui.add(
                            egui::Slider::new(&mut options.render_distance, 1..=max_rd)
                                .suffix(" chunks"),
                        );
                        ui.end_row();

                        ui.label("Mouse sensitivity:");
                        ui.add(
                            egui::Slider::new(&mut options.mouse_sensitivity, 0.05..=2.0)
                                .step_by(0.05),
                        );
                        ui.end_row();

                        ui.label("Field of view:");
                        ui.add(egui::Slider::new(&mut options.fov, 30.0..=120.0).suffix("°"));
                        ui.end_row();
                    });

                ui.add_space(30.0);

                if ui
                    .add_sized(
                        egui::vec2(200.0, 40.0),
                        egui::Button::new(egui::RichText::new("← Back").size(18.0)),
                    )
                    .clicked()
                {
                    back = true;
                }
            });
        });

    back
}

pub fn draw_crosshair(ctx: &egui::Context, center: egui::Pos2) {
    let crosshair_size = 10.0;
    let crosshair_thickness = 2.0;
    let crosshair_color = egui::Color32::WHITE;

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));

    painter.line_segment(
        [
            egui::pos2(center.x - crosshair_size, center.y),
            egui::pos2(center.x + crosshair_size, center.y),
        ],
        egui::Stroke::new(crosshair_thickness, crosshair_color),
    );

    painter.line_segment(
        [
            egui::pos2(center.x, center.y - crosshair_size),
            egui::pos2(center.x, center.y + crosshair_size),
        ],
        egui::Stroke::new(crosshair_thickness, crosshair_color),
    );
}
