use wgpu::util::DeviceExt;

use noise::OpenSimplex;

use crate::{
    block::BlockVertex,
    chunk::{CHUNK_X_SIZE, CHUNK_Y_SIZE, CHUNK_Z_SIZE, Chunk, block_index},
    texture_atlas::TextureAtlas,
};

use std::collections::{HashSet, VecDeque};

#[cfg(not(target_arch = "wasm32"))]
struct FrameBudget {
    start:  std::time::Instant,
    millis: u128,
}
#[cfg(not(target_arch = "wasm32"))]
impl FrameBudget {
    fn new(millis: u128) -> Self { Self { start: std::time::Instant::now(), millis } }
    fn has_time(&mut self) -> bool { self.start.elapsed().as_millis() < self.millis }
}

#[cfg(target_arch = "wasm32")]
struct FrameBudget {
    remaining: usize,
}
#[cfg(target_arch = "wasm32")]
impl FrameBudget {
    fn new(millis: u128) -> Self { Self { remaining: millis as usize } }
    fn has_time(&mut self) -> bool {
        if self.remaining == 0 { return false; }
        self.remaining -= 1;
        true
    }
}

struct ChunkGenRequest {
    pos: [i32; 2],
    left_blocks: Option<Vec<Vec<u32>>>,
    right_blocks: Option<Vec<Vec<u32>>>,
    back_blocks: Option<Vec<Vec<u32>>>,
    front_blocks: Option<Vec<Vec<u32>>>,
}

struct ReadyChunk {
    chunk: Chunk,
}

fn build_chunk_with_boundaries(
    req: ChunkGenRequest,
    noise_gen: OpenSimplex,
) -> ReadyChunk {
    let chunk = Chunk::new_with_boundaries(
        req.pos,
        noise_gen,
        req.back_blocks,
        req.front_blocks,
        req.left_blocks,
        req.right_blocks,
    );
    ReadyChunk { chunk }
}

#[cfg(not(target_arch = "wasm32"))]
struct ChunkGenThread {
    tx_request: std::sync::mpsc::Sender<ChunkGenRequest>,
    rx_ready: std::sync::mpsc::Receiver<ReadyChunk>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkGenThread {
    fn new(noise_gen: OpenSimplex) -> Self {
        let (tx_request, rx_request) = std::sync::mpsc::channel::<ChunkGenRequest>();
        let (tx_ready, rx_ready) = std::sync::mpsc::channel::<ReadyChunk>();

        let rx_shared = std::sync::Arc::new(std::sync::Mutex::new(rx_request));

        let n_workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).max(1).min(4))
            .unwrap_or(2);

        for idx in 0..n_workers {
            let rx = std::sync::Arc::clone(&rx_shared);
            let tx = tx_ready.clone();
            std::thread::Builder::new()
                .name(format!("chunk-gen-{}", idx))
                .spawn(move || {
                    loop {
                        let req = match rx.lock().unwrap().recv() {
                            Ok(r) => r,
                            Err(_) => break,
                        };
                        let ready = build_chunk_with_boundaries(req, noise_gen);
                        let _ = tx.send(ready);
                    }
                })
                .expect("failed to spawn chunk-gen thread");
        }

        Self {
            tx_request,
            rx_ready,
        }
    }

    fn request(&mut self, req: ChunkGenRequest) {
        let _ = self.tx_request.send(req);
    }

    fn poll_ready(&mut self, limit: usize) -> Vec<ReadyChunk> {
        let mut out = Vec::with_capacity(limit);
        while out.len() < limit {
            match self.rx_ready.try_recv() {
                Ok(r) => out.push(r),
                Err(_) => break,
            }
        }
        out
    }
}

#[cfg(target_arch = "wasm32")]
struct ChunkGenThread {
    noise_gen: OpenSimplex,
    pending: VecDeque<ChunkGenRequest>,
}

#[cfg(target_arch = "wasm32")]
impl ChunkGenThread {
    fn new(noise_gen: OpenSimplex) -> Self {
        Self {
            noise_gen,
            pending: VecDeque::new(),
        }
    }

    fn request(&mut self, req: ChunkGenRequest) {
        self.pending.push_back(req);
    }

    fn poll_ready(&mut self, limit: usize) -> Vec<ReadyChunk> {
        let mut out = Vec::new();
        while out.len() < limit {
            if let Some(req) = self.pending.pop_front() {
                out.push(build_chunk_with_boundaries(req, self.noise_gen));
            } else {
                break;
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct ChunkBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub indices_buffer: wgpu::Buffer,
    pub num_elements: u32,
    pub transparent_indices_buffer: Option<wgpu::Buffer>,
    pub transparent_num_elements: u32,
    vertex_capacity: usize,
    index_capacity: usize,
    transparent_index_capacity: usize,
}

impl ChunkBuffer {
    pub fn new(
        device: &wgpu::Device,
        vertices: Vec<BlockVertex>,
        indices: Vec<u32>,
        num_elements: u32,
        transparent_indices: Vec<u32>,
        transparent_num_elements: u32,
    ) -> Self {
        let vertex_capacity = vertices.len();
        let index_capacity = indices.len();
        let transparent_index_capacity = transparent_indices.len();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunkbuffer vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let indices_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunkbuffer indices buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        let transparent_indices_buffer = if transparent_indices.is_empty() {
            None
        } else {
            Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunkbuffer transparent indices buffer"),
                contents: bytemuck::cast_slice(&transparent_indices),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            }))
        };

        Self {
            vertex_buffer,
            indices_buffer,
            num_elements,
            transparent_indices_buffer,
            transparent_num_elements,
            vertex_capacity,
            index_capacity,
            transparent_index_capacity,
        }
    }

    pub fn update_or_recreate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: Vec<BlockVertex>,
        indices: Vec<u32>,
        num_elements: u32,
        transparent_indices: Vec<u32>,
        transparent_num_elements: u32,
    ) {
        if vertices.len() > self.vertex_capacity || indices.len() > self.index_capacity {
            self.vertex_capacity = (vertices.len() as f32 * 1.5) as usize;
            self.index_capacity = (indices.len() as f32 * 1.5) as usize;

            let vertex_buffer_size =
                (self.vertex_capacity * std::mem::size_of::<BlockVertex>()) as u64;
            let index_buffer_size = (self.index_capacity * std::mem::size_of::<u32>()) as u64;

            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunkbuffer vertex buffer"),
                size: vertex_buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.indices_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunkbuffer indices buffer"),
                size: index_buffer_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.indices_buffer, 0, bytemuck::cast_slice(&indices));
        self.num_elements = num_elements;

        if transparent_indices.is_empty() {
            self.transparent_indices_buffer = None;
            self.transparent_num_elements = 0;
            self.transparent_index_capacity = 0;
        } else {
            let needs_new = transparent_indices.len() > self.transparent_index_capacity
                || self.transparent_indices_buffer.is_none();
            if needs_new {
                self.transparent_index_capacity =
                    (transparent_indices.len() as f32 * 1.5) as usize;
                let buf_size =
                    (self.transparent_index_capacity * std::mem::size_of::<u32>()) as u64;
                self.transparent_indices_buffer = Some(device.create_buffer(
                    &wgpu::BufferDescriptor {
                        label: Some("chunkbuffer transparent indices buffer"),
                        size: buf_size,
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    },
                ));
            }
            queue.write_buffer(
                self.transparent_indices_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(&transparent_indices),
            );
            self.transparent_num_elements = transparent_num_elements;
        }
    }
}

pub struct World {
    pub chunks: Vec<Chunk>,
    pub chunk_buffers: Vec<ChunkBuffer>,

    pub noise_gen: OpenSimplex,

    pub texture_atlas: TextureAtlas,

    pending_chunks: HashSet<[i32; 2]>,

    gen_thread: ChunkGenThread,

    seam_fix_queue: VecDeque<[i32; 2]>,

    ready_queue: VecDeque<ReadyChunk>,

    pub render_distance: i32,
}

impl World {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, seed: u32) -> Self {
        let noise_gen = OpenSimplex::new(seed);

        let mut chunks = Vec::new();
        let mut chunk_buffers = Vec::new();

        const WORLD_SIZE: i32 = 5;

        for x in -WORLD_SIZE..WORLD_SIZE {
            for y in -WORLD_SIZE..WORLD_SIZE {
                let base_chunk = Chunk::new([x, y], noise_gen);
                chunks.push(base_chunk);
            }
        }

        for i in 0..chunks.len() {
            let pos = chunks[i].pos;

            let back  = chunks.iter().find(|c| c.pos == [pos[0], pos[1] - 1])
                .map(|c| Self::get_boundary_blocks(c, 1));
            let front = chunks.iter().find(|c| c.pos == [pos[0], pos[1] + 1])
                .map(|c| Self::get_boundary_blocks(c, 0));
            let left  = chunks.iter().find(|c| c.pos == [pos[0] - 1, pos[1]])
                .map(|c| Self::get_boundary_blocks(c, 3));
            let right = chunks.iter().find(|c| c.pos == [pos[0] + 1, pos[1]])
                .map(|c| Self::get_boundary_blocks(c, 2));

            chunks[i].boundary = [back, front, left, right];
            chunks[i].regenerate_mesh();

            let chunk_buffer = ChunkBuffer::new(
                device,
                std::mem::take(&mut chunks[i].mesh.vertices),
                std::mem::take(&mut chunks[i].mesh.indices),
                chunks[i].mesh.num_elements,
                std::mem::take(&mut chunks[i].mesh.transparent_indices),
                chunks[i].mesh.transparent_num_elements,
            );
            chunk_buffers.push(chunk_buffer);
        }

        let gen_thread = ChunkGenThread::new(noise_gen);

        Self {
            chunks,
            chunk_buffers,

            noise_gen,

            texture_atlas: TextureAtlas::new(device, queue),

            pending_chunks: HashSet::new(),
            gen_thread,
            seam_fix_queue: VecDeque::new(),
            ready_queue: VecDeque::new(),
            render_distance: if cfg!(target_arch = "wasm32") { 4 } else { 8 },
        }
    }

    pub fn break_block(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: [i32; 3],
    ) -> Option<u32> {
        if let Some((chunk_index, _)) = self
            .chunks
            .iter_mut()
            .enumerate()
            .find(|(_, c)| c.contains_block(pos))
        {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);

            let block_type = {
                let (lx, ly, lz) = (
                    local_pos[0] as usize,
                    local_pos[1] as usize,
                    local_pos[2] as usize,
                );
                let mat = self.chunks[chunk_index].get_block_type(lx, ly, lz);
                if mat != 0 { Some(mat) } else { None }
            };

            self.chunks[chunk_index].break_block(pos);
            self.update_chunk_mesh(device, queue, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }

            block_type
        } else {
            None
        }
    }

    pub fn place_block(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pos: [i32; 3],
        selected_block: u32,
    ) {
        if let Some((chunk_index, _)) = self
            .chunks
            .iter_mut()
            .enumerate()
            .find(|(_, c)| c.contains_position(pos))
        {
            let chunk_pos = self.chunks[chunk_index].pos;
            let local_pos = self.chunks[chunk_index].get_local_pos(pos);

            self.chunks[chunk_index].place_block(pos, selected_block);
            self.update_chunk_mesh(device, queue, chunk_index);

            if local_pos[0] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] - 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[0] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0] + 1, chunk_pos[1]]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 0 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] - 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
            if local_pos[2] == 15 {
                if let Some(idx) = self.find_chunk([chunk_pos[0], chunk_pos[1] + 1]) {
                    self.update_chunk_mesh(device, queue, idx);
                }
            }
        }
    }

    fn find_chunk(&self, pos: [i32; 2]) -> Option<usize> {
        self.chunks.iter().position(|c| c.pos == pos)
    }

    pub fn update_chunks(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        player_chunk: [i32; 2],
    ) {
        let render_distance = self.render_distance.max(1);
        let unload_distance = render_distance + 3;
        let max_in_flight: usize = if cfg!(target_arch = "wasm32") {
            24
        } else {
            200
        };

        let fresh = self.gen_thread.poll_ready(8);
        self.ready_queue.extend(fresh);

        let mut budget = FrameBudget::new(2);
        while budget.has_time() {
            let Some(mut chunk) = self.ready_queue.pop_front().map(|r| r.chunk) else {
                break;
            };

            let pos = chunk.pos;
            self.pending_chunks.remove(&pos);

            let dx = (pos[0] - player_chunk[0]).abs();
            let dz = (pos[1] - player_chunk[1]).abs();
            if dx > unload_distance || dz > unload_distance {
                continue;
            }

            let chunk_buffer = ChunkBuffer::new(
                device,
                std::mem::take(&mut chunk.mesh.vertices),
                std::mem::take(&mut chunk.mesh.indices),
                chunk.mesh.num_elements,
                std::mem::take(&mut chunk.mesh.transparent_indices),
                chunk.mesh.transparent_num_elements,
            );
            self.chunks.push(chunk);
            self.chunk_buffers.push(chunk_buffer);

            for npos in [
                [pos[0] - 1, pos[1]],
                [pos[0] + 1, pos[1]],
                [pos[0], pos[1] - 1],
                [pos[0], pos[1] + 1],
            ] {
                if !self.seam_fix_queue.contains(&npos) {
                    self.seam_fix_queue.push_back(npos);
                }
            }
        }

        let mut seam_budget = FrameBudget::new(2);
        while seam_budget.has_time() {
            let Some(npos) = self.seam_fix_queue.pop_front() else {
                break;
            };
            if let Some(idx) = self.find_chunk(npos) {
                self.update_chunk_mesh(device, queue, idx);
            }
        }

        let mut i = self.chunks.len();
        while i > 0 {
            i -= 1;
            let dx = (self.chunks[i].pos[0] - player_chunk[0]).abs();
            let dz = (self.chunks[i].pos[1] - player_chunk[1]).abs();
            if dx > unload_distance || dz > unload_distance {
                self.chunks.remove(i);
                self.chunk_buffers.remove(i);
            }
        }
        self.pending_chunks.retain(|pos| {
            let dx = (pos[0] - player_chunk[0]).abs();
            let dz = (pos[1] - player_chunk[1]).abs();
            dx <= unload_distance && dz <= unload_distance
        });

        let mut to_load: Vec<([i32; 2], i32)> = Vec::new();
        for dx in -render_distance..=render_distance {
            for dz in -render_distance..=render_distance {
                if dx * dx + dz * dz > render_distance * render_distance {
                    continue;
                }
                let pos = [player_chunk[0] + dx, player_chunk[1] + dz];
                if self.pending_chunks.contains(&pos) || self.chunks.iter().any(|c| c.pos == pos) {
                    continue;
                }
                to_load.push((pos, dx * dx + dz * dz));
            }
        }
        to_load.sort_by_key(|(_, d)| *d);

        let available = max_in_flight.saturating_sub(self.pending_chunks.len());
        for (pos, _) in to_load.into_iter().take(available) {
            let left_blocks = self
                .find_chunk([pos[0] - 1, pos[1]])
                .map(|i| Self::get_boundary_blocks(&self.chunks[i], 3));
            let right_blocks = self
                .find_chunk([pos[0] + 1, pos[1]])
                .map(|i| Self::get_boundary_blocks(&self.chunks[i], 2));
            let back_blocks = self
                .find_chunk([pos[0], pos[1] - 1])
                .map(|i| Self::get_boundary_blocks(&self.chunks[i], 1));
            let front_blocks = self
                .find_chunk([pos[0], pos[1] + 1])
                .map(|i| Self::get_boundary_blocks(&self.chunks[i], 0));
            self.pending_chunks.insert(pos);
            self.gen_thread.request(ChunkGenRequest {
                pos,
                left_blocks,
                right_blocks,
                back_blocks,
                front_blocks,
            });
        }
    }

    fn get_boundary_blocks(chunk: &Chunk, face: usize) -> Vec<Vec<u32>> {
        let (outer, inner) = match face {
            0 | 1 => (CHUNK_X_SIZE, CHUNK_Y_SIZE),
            _ => (CHUNK_Z_SIZE, CHUNK_Y_SIZE),
        };
        let mut blocks = vec![vec![0u32; inner]; outer];

        match face {
            // face 0: back face
            0 => {
                for x in 0..CHUNK_X_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[x][y] = chunk.block_types[block_index(x, y, 0)];
                    }
                }
            }
            // face 1: front face
            1 => {
                for x in 0..CHUNK_X_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[x][y] = chunk.block_types[block_index(x, y, CHUNK_Z_SIZE - 1)];
                    }
                }
            }
            // face 2: left face
            2 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.block_types[block_index(0, y, z)];
                    }
                }
            }
            // face 3: right face
            3 => {
                for z in 0..CHUNK_Z_SIZE {
                    for y in 0..CHUNK_Y_SIZE {
                        blocks[z][y] = chunk.block_types[block_index(CHUNK_X_SIZE - 1, y, z)];
                    }
                }
            }
            _ => {}
        }

        blocks
    }

    fn update_chunk_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        chunk_index: usize,
    ) {
        let pos = self.chunks[chunk_index].pos;

        let back  = self.find_chunk([pos[0], pos[1] - 1])
            .map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 1));
        let front = self.find_chunk([pos[0], pos[1] + 1])
            .map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 0));
        let left  = self.find_chunk([pos[0] - 1, pos[1]])
            .map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 3));
        let right = self.find_chunk([pos[0] + 1, pos[1]])
            .map(|idx| Self::get_boundary_blocks(&self.chunks[idx], 2));

        self.chunks[chunk_index].boundary = [back, front, left, right];
        self.chunks[chunk_index].regenerate_mesh();

        self.chunk_buffers[chunk_index].update_or_recreate(
            device,
            queue,
            std::mem::take(&mut self.chunks[chunk_index].mesh.vertices),
            std::mem::take(&mut self.chunks[chunk_index].mesh.indices),
            self.chunks[chunk_index].mesh.num_elements,
            std::mem::take(&mut self.chunks[chunk_index].mesh.transparent_indices),
            self.chunks[chunk_index].mesh.transparent_num_elements,
        );
    }
}
