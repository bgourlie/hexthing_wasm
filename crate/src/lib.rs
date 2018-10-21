#![feature(use_extern_macros)]

#[macro_use]
extern crate cfg_if;
extern crate wasm_bindgen;
extern crate js_sys;
extern crate specs;
extern crate web_sys;
extern crate rand;

use rand::Rng;
use rand::distributions::{Distribution};
use specs::prelude::*;
use specs::storage::HashMapStorage;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use js_sys::{WebAssembly};
use web_sys::{console, WebGlProgram, WebGl2RenderingContext, WebGlShader};


macro_rules! console_log {
    ($($t:tt)*) => (console::log_1(JsValue::from_str(&format_args!($($t)*).to_string()).as_ref());)
}

cfg_if! {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function to get better error messages if we ever panic.
    if #[cfg(feature = "console_error_panic_hook")] {
        extern crate console_error_panic_hook;
        use console_error_panic_hook::set_once as set_panic_hook;
    }
}

cfg_if! {
    // When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
    // allocator.
    if #[cfg(feature = "wee_alloc")] {
        extern crate wee_alloc;
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}
const TAU: f32 = 2. * std::f32::consts::PI;

#[derive(Debug)]
struct ClusterBomb {
    fuse: usize,
}
impl Component for ClusterBomb {
    // This uses `HashMapStorage`, because only some entities are cluster bombs.
    type Storage = HashMapStorage<Self>;
}

#[derive(Debug)]
struct Shrapnel {
    durability: usize,
}
impl Component for Shrapnel {
    // This uses `HashMapStorage`, because only some entities are shrapnels.
    type Storage = HashMapStorage<Self>;
}

#[derive(Debug, Clone)]
struct Pos(f32, f32);
impl Component for Pos {
    // This uses `VecStorage`, because all entities have a position.
    type Storage = VecStorage<Self>;
}

#[derive(Debug)]
struct Vel(f32, f32);
impl Component for Vel {
    // This uses `DenseVecStorage`, because nearly all entities have a velocity.
    type Storage = DenseVecStorage<Self>;
}

struct ClusterBombSystem;
impl<'a> System<'a> for ClusterBombSystem {
    type SystemData = (
        Entities<'a>,
        WriteStorage<'a, ClusterBomb>,
        ReadStorage<'a, Pos>,
        // Allows lazily adding and removing components to entities
        // or executing arbitrary code with world access lazily via `execute`.
        Read<'a, LazyUpdate>,
    );

    fn run(&mut self, (entities, mut bombs, positions, updater): Self::SystemData) {
        // Join components in potentially parallel way using rayon.
        (&entities, &mut bombs, &positions)
            .join()
            .for_each(|(entity, bomb, position)| {
                let mut rng = rand::thread_rng();

                if bomb.fuse == 0 {
                    let _ = entities.delete(entity);
                    for _ in 0..9 {
                        let shrapnel = entities.create();
                        updater.insert(
                            shrapnel,
                            Shrapnel {
                                durability: rng.gen_range(10, 20),
                            },
                        );
                        updater.insert(shrapnel, position.clone());
                        let angle: f32 = rng.gen::<f32>() * TAU;
                        updater.insert(shrapnel, Vel(angle.sin(), angle.cos()));
                    }
                } else {
                    bomb.fuse -= 1;
                }
            });
    }
}

struct PhysicsSystem;
impl<'a> System<'a> for PhysicsSystem {
    type SystemData = (WriteStorage<'a, Pos>, ReadStorage<'a, Vel>);

    fn run(&mut self, (mut pos, vel): Self::SystemData) {
        (&mut pos, &vel).join().for_each(|(pos, vel)| {
            pos.0 += vel.0;
            pos.1 += vel.1;
        });
    }
}

struct ShrapnelSystem;
impl<'a> System<'a> for ShrapnelSystem {
    type SystemData = (Entities<'a>, WriteStorage<'a, Shrapnel>);

    fn run(&mut self, (entities, mut shrapnels): Self::SystemData) {
        (&entities, &mut shrapnels)
            .join()
            .for_each(|(entity, shrapnel)| {
                if shrapnel.durability == 0 {
                    let _ = entities.delete(entity);
                } else {
                    shrapnel.durability -= 1;
                }
            });
    }
}

#[wasm_bindgen]
pub fn draw() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| ())
        .unwrap();

    let context = canvas
        .get_context("webgl2")
        .unwrap()
        .unwrap()
        .dyn_into::<WebGl2RenderingContext>()
        .unwrap();

    let vert_shader = compile_shader(
        &context,
        WebGl2RenderingContext::VERTEX_SHADER,
        r#"
        attribute vec4 position;
        void main() {
            gl_Position = position;
        }
    "#,
    ).unwrap();
    let frag_shader = compile_shader(
        &context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        r#"
        void main() {
            gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
        }
    "#,
    ).unwrap();
    let program = link_program(&context, [vert_shader, frag_shader].iter()).unwrap();
    context.use_program(Some(&program));

    let vertices: [f32; 9] = [-0.7, -0.7, 0.0, 0.7, -0.7, 0.0, 0.0, 0.7, 0.0];
    let memory_buffer = wasm_bindgen::memory().dyn_into::<WebAssembly::Memory>().unwrap().buffer();
    let vertices_location = vertices.as_ptr() as u32 / 4;
    let vert_array = js_sys::Float32Array::new(&memory_buffer).subarray(
        vertices_location,
        vertices_location + vertices.len() as u32,
    );

    let buffer = context.create_buffer().unwrap();
    context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));
    context.buffer_data_with_array_buffer_view(
        WebGl2RenderingContext::ARRAY_BUFFER,
        vert_array.as_ref(),
        WebGl2RenderingContext::STATIC_DRAW,
    );
    context.vertex_attrib_pointer_with_i32(0, 3, WebGl2RenderingContext::FLOAT, false, 0, 0);
    context.enable_vertex_attrib_array(0);

    context.clear_color(0.0, 0.0, 0.0, 1.0);
    context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

    context.draw_arrays(
        WebGl2RenderingContext::TRIANGLES,
        0,
        (vertices.len() / 3) as i32,
    );

    let mut world = World::new();

    let mut dispatcher = DispatcherBuilder::new()
        .with(PhysicsSystem, "physics", &[])
        .with(ClusterBombSystem, "cluster_bombs", &[])
        .with(ShrapnelSystem, "shrapnels", &[])
        .build();

    dispatcher.setup(&mut world.res);

    world
        .create_entity()
        .with(Pos(0., 0.))
        .with(ClusterBomb { fuse: 3 })
        .build();

    let mut step = 0;
    loop {
        step += 1;
        let mut entities = 0;
        {
            // Simple console rendering
            let positions = world.read_storage::<Pos>();
            const WIDTH: usize = 10;
            const HEIGHT: usize = 10;
            const SCALE: f32 = 1. / 4.;
            let mut screen = [[0; WIDTH]; HEIGHT];
            for entity in world.entities().join() {
                if let Some(pos) = positions.get(entity) {
                    let x = (pos.0 * SCALE + WIDTH as f32 / 2.).floor() as usize;
                    let y = (pos.1 * SCALE + HEIGHT as f32 / 2.).floor() as usize;
                    if x < WIDTH && y < HEIGHT {
                        screen[x][y] += 1;
                    }
                }
                entities += 1;
            }
            console_log!("Step: {}, Entities: {}", step, entities);
            for row in &screen {
                let mut line = String::with_capacity(10);
                for cell in row {
                    line.push(format!("{}", cell).chars().next().unwrap());
                }
                console_log!("{}", line);
            }
        }
        if entities == 0 {
            break;
        }

        dispatcher.dispatch(&world.res);

        // Maintain dynamically added and removed entities in dispatch.
        // This is what actually executes changes done by `LazyUpdate`.
        world.maintain();
    }
}
pub fn compile_shader(
    context: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
        {
            Ok(shader)
        } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "Unknown error creating shader".into()))
    }
}

pub fn link_program<'a, T: IntoIterator<Item = &'a WebGlShader>>(
    context: &WebGl2RenderingContext,
    shaders: T,
) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    for shader in shaders {
        context.attach_shader(&program, shader)
    }
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
        {
            Ok(program)
        } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| "Unknown error creating program object".into()))
    }
}
