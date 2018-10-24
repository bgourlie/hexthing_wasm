#![feature(use_extern_macros)]

#[macro_use]
extern crate cfg_if;
extern crate fnv;
extern crate js_sys;
extern crate nalgebra;
extern crate rand;
extern crate specs;
extern crate wasm_bindgen;
extern crate web_sys;

use fnv::FnvHashMap;
use js_sys::{Float32Array, WebAssembly};
use nalgebra::{Matrix4, Translation, Vector3};
use specs::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    console, HtmlCanvasElement, WebGl2RenderingContext, WebGlProgram, WebGlShader,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

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
struct ProjectionMatrix {
    perspective: Matrix4<f32>,
}

impl Default for ProjectionMatrix {
    fn default() -> Self {
        ProjectionMatrix {
            perspective: Matrix4::new_perspective(0.0, 0.0, 0.0, 0.0),
        }
    }
}

struct RenderSystem {
    gl: WebGl2RenderingContext,
    renderers: FnvHashMap<String, Renderable>,
}

impl<'a> System<'a> for RenderSystem {
    type SystemData = (
        Write<'a, ProjectionMatrix>, // Ideally would be Read, see https://github.com/rustwasm/wasm-bindgen/issues/978
        ReadStorage<'a, Pos>,
        ReadStorage<'a, Rendered>,
    );

    fn run(&mut self, (mut projection_matrix, positions, rendereds): Self::SystemData) {
        self.gl.clear(
            WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT,
        );
        (&positions, &rendereds)
            .join()
            .for_each(|(Pos(position), rendered)| {
                let renderer = self.renderers.get(&rendered.renderable_id).unwrap();

                self.gl.bind_vertex_array(Some(&renderer.vao));

                for input in &renderer.definition.inputs {
                    self.gl.buffer_data_with_array_buffer_view(
                        input.buffer_type,
                        input.vertices.as_ref(),
                        WebGl2RenderingContext::STATIC_DRAW,
                    );
                }

                let mut model_view_matrix = Translation::from_vector(*position).to_homogeneous();

                self.gl.use_program(Some(&renderer.program));

                self.gl.uniform_matrix4fv_with_f32_array(
                    Some(&renderer.projection_matrix_location),
                    false,
                    projection_matrix.perspective.as_mut_slice(),
                );

                self.gl.uniform_matrix4fv_with_f32_array(
                    Some(&renderer.model_view_matrix_location),
                    false,
                    model_view_matrix.as_mut_slice(),
                );
                self.gl.draw_arrays(
                    renderer.definition.draw_mode,
                    0,
                    renderer.definition.vertices_to_render,
                );
                self.gl.bind_vertex_array(None);
            })
    }
}

#[derive(Debug, Default)]
struct RenderSystemBuilder {
    canvas: Option<HtmlCanvasElement>,
    definitions: Vec<RenderableDefinition>,
}

impl RenderSystemBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn with_canvas(mut self, canvas: HtmlCanvasElement) -> Self {
        self.canvas = Some(canvas);
        self
    }

    fn register(mut self, definition: RenderableDefinition) -> Self {
        self.definitions.push(definition);
        self
    }

    fn build(self) -> Result<RenderSystem, String> {
        if let Some(canvas) = self.canvas {
            let gl = canvas
                .get_context("webgl2")
                .unwrap()
                .unwrap()
                .dyn_into::<WebGl2RenderingContext>()
                .unwrap();

            let mut renderers = FnvHashMap::default();

            for definition in self.definitions {
                if renderers.contains_key(&definition.id) {
                    return Err(
                        format!("Multiple renderers registered with id {}", definition.id)
                            .to_owned(),
                    );
                }

                let renderer_id = definition.id.clone();
                let renderer = Self::compile(&gl, definition).unwrap();
                renderers.insert(renderer_id, renderer);
            }

            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);
            gl.enable(WebGl2RenderingContext::DEPTH_TEST);
            gl.depth_func(WebGl2RenderingContext::LEQUAL);

            Ok(RenderSystem { gl, renderers })
        } else {
            Err("No canvas specified".to_owned())
        }
    }

    fn compile(
        gl: &WebGl2RenderingContext,
        definition: RenderableDefinition,
    ) -> Result<Renderable, String> {
        console_log!("Compiling render {}", definition.id);
        let vert_shader = Self::compile_shader(
            gl,
            WebGl2RenderingContext::VERTEX_SHADER,
            &definition.vertex_shader,
        )
        .unwrap();

        let frag_shader = Self::compile_shader(
            gl,
            WebGl2RenderingContext::FRAGMENT_SHADER,
            &definition.fragment_shader,
        )
        .unwrap();

        let program = Self::link_program(gl, [vert_shader, frag_shader].iter()).unwrap();

        let projection_matrix_location = gl
            .get_uniform_location(&program, "uProjectionMatrix")
            .unwrap();
        let model_view_matrix_location = gl
            .get_uniform_location(&program, "uModelViewMatrix")
            .unwrap();

        let vao = gl.create_vertex_array().unwrap();
        let buffer = gl.create_buffer().unwrap();

        gl.bind_vertex_array(Some(&vao));

        for input in &definition.inputs {
            gl.enable_vertex_attrib_array(input.location);
            gl.bind_buffer(input.buffer_type, Some(&buffer));
            gl.vertex_attrib_pointer_with_i32(
                input.location,
                input.num_components,
                input.buffer_data_type,
                false,
                0,
                0,
            );
        }

        gl.bind_vertex_array(None);

        Ok(Renderable {
            definition,
            program,
            vao,
            projection_matrix_location,
            model_view_matrix_location,
        })
    }

    fn compile_shader(
        gl: &WebGl2RenderingContext,
        shader_type: u32,
        source: &str,
    ) -> Result<WebGlShader, String> {
        let shader = gl
            .create_shader(shader_type)
            .ok_or_else(|| String::from("Unable to create shader object"))?;
        gl.shader_source(&shader, source);
        gl.compile_shader(&shader);

        if gl
            .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            Ok(shader)
        } else {
            Err(gl
                .get_shader_info_log(&shader)
                .unwrap_or_else(|| "Unknown error creating shader".into()))
        }
    }

    fn link_program<'a, T: IntoIterator<Item = &'a WebGlShader>>(
        gl: &WebGl2RenderingContext,
        shaders: T,
    ) -> Result<WebGlProgram, String> {
        let program = gl
            .create_program()
            .ok_or_else(|| String::from("Unable to create shader object"))?;
        for shader in shaders {
            gl.attach_shader(&program, shader)
        }
        gl.link_program(&program);

        if gl
            .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            Ok(program)
        } else {
            Err(gl
                .get_program_info_log(&program)
                .unwrap_or_else(|| "Unknown error creating program object".into()))
        }
    }
}

#[derive(Debug)]
struct RenderableDefinition {
    id: String,
    fragment_shader: String,
    vertex_shader: String,
    inputs: Vec<InputDescriptor>,
    draw_mode: u32,
    vertices_to_render: i32,
}

#[derive(Debug)]
struct InputDescriptor {
    location: u32,
    buffer_type: u32,
    buffer_data_type: u32,
    num_components: i32,
    vertices: Float32Array,
}

#[derive(Debug)]
struct Renderable {
    definition: RenderableDefinition,
    program: WebGlProgram,
    vao: WebGlVertexArrayObject,
    projection_matrix_location: WebGlUniformLocation,
    model_view_matrix_location: WebGlUniformLocation,
}

#[derive(Debug)]
struct Rendered {
    renderable_id: String,
}

impl Component for Rendered {
    type Storage = DenseVecStorage<Self>;
}

#[derive(Debug, Clone)]
struct Pos(Vector3<f32>);
impl Component for Pos {
    type Storage = VecStorage<Self>;
}

#[wasm_bindgen]
pub fn draw() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| ())
        .unwrap();

    let vertices: [f32; 16] = [
        0.0_f32,
        0.0_f32,
        0.8660254037844387_f32,
        -0.5_f32,
        0.8660254037844387_f32,
        0.5_f32,
        0.0_f32,
        1.0_f32,
        -0.8660254037844387_f32,
        0.5_f32,
        -0.8660254037844387_f32,
        -0.5_f32,
        0.0_f32,
        -1.0_f32,
        0.8660254037844387_f32,
        -0.5_f32,
    ];

    let memory_buffer = wasm_bindgen::memory()
        .dyn_into::<WebAssembly::Memory>()
        .unwrap()
        .buffer();

    let vertices_location = vertices.as_ptr() as u32 / 4;

    let vert_array = Float32Array::new(&memory_buffer)
        .subarray(vertices_location, vertices_location + vertices.len() as u32);

    let render_definition = RenderableDefinition {
        id: "hexTile".to_owned(),
        draw_mode: WebGl2RenderingContext::TRIANGLE_FAN,
        vertex_shader: r#"#version 300 es
            layout(location = 0) in vec4 position;

            uniform mat4 uModelViewMatrix;
            uniform mat4 uProjectionMatrix;

            void main() {
              gl_Position = uProjectionMatrix * uModelViewMatrix * position;
            }"#
        .to_owned(),
        fragment_shader: r#"#version 300 es
            precision mediump float;
            out vec4 fragColor;

            void main() {
              fragColor = vec4(1.0, 1.0, 1.0, 1.0);
            }"#
        .to_owned(),
        inputs: vec![InputDescriptor {
            location: 0,
            buffer_type: WebGl2RenderingContext::ARRAY_BUFFER,
            buffer_data_type: WebGl2RenderingContext::FLOAT,
            num_components: 2,
            vertices: vert_array,
        }],
        vertices_to_render: 8,
    };

    let render_system = RenderSystemBuilder::new()
        .with_canvas(canvas)
        .register(render_definition)
        .build()
        .unwrap();

    let mut world = World::new();

    let fov = 45.0 * std::f32::consts::PI / 180.0;
    let aspect_ratio = 1.0;
    let z_near = 0.1;
    let z_far = 100.0;

    world.add_resource(ProjectionMatrix {
        perspective: Matrix4::new_perspective(aspect_ratio, fov, z_near, z_far),
    });

    let mut dispatcher = DispatcherBuilder::new()
        .with_thread_local(render_system)
        .build();

    dispatcher.setup(&mut world.res);

    world
        .create_entity()
        .with(Pos(Vector3::new(0.0, 0.0, -8.0)))
        .with(Rendered {
            renderable_id: "hexTile".to_owned(),
        })
        .build();

    dispatcher.dispatch(&world.res);

    // Maintain dynamically added and removed entities in dispatch.
    // This is what actually executes changes done by `LazyUpdate`.
    world.maintain();
}
