use std::fs;
use std::io;
use std::sync::Arc;
use std::time::Instant;
use std::env;

use aetheria::animation::{BodyMotion, Orbit, RotationAxis};
use aetheria::assets::load_obj;
use aetheria::camera::Camera;
use aetheria::color::Color;
use aetheria::framebuffer::Framebuffer;
use aetheria::math::{Mat4, Vec3};
use aetheria::pipeline::render_mesh_with_shader;
use aetheria::orbit_camera::OrbitCameraController;
use aetheria::scene::{BodyId, BodyPose, BodyPoses};
use aetheria::shader::ShaderUniforms;
use aetheria::shaders::cryon::CryonShader;
use aetheria::shaders::nymara::{NymaraRingShader, NymaraShader};
use aetheria::shaders::selene::SeleneShader;
use aetheria::shaders::solaris::SolarisShader;
use aetheria::shaders::solene::SoleneShader;
use aetheria::shaders::umbra::UmbraShader;
use aetheria::shaders::veridis::VeridisShader;
use aetheria::shaders::vesper::VesperShader;
use aetheria::shadows::ShadowScene;
#[cfg(feature = "interactive")]
use aetheria::window::CpuWindow;

fn main() -> io::Result<()> {
    let mut interactive = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--offline" => {}
            "--interactive" => interactive = true,
            _ => {
                eprintln!("Uso: aetheria [--offline] [--interactive]");
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "argumento desconocido"));
            }
        }
    }
    if interactive {
        #[cfg(feature = "interactive")]
        {
            return run_app(true);
        }
        #[cfg(not(feature = "interactive"))]
        {
            eprintln!("El modo interactivo requiere compilar con: cargo run --features interactive -- --interactive");
            return Err(io::Error::new(io::ErrorKind::Unsupported, "feature interactive no habilitada"));
        }
    }
    run_app(false)
}

fn run_app(_interactive: bool) -> io::Result<()> {
    let solaris = load_obj("assets/models/obj/solaris.obj").map_err(io::Error::other)?;
    let vesper = load_obj("assets/models/obj/vesper.obj").map_err(io::Error::other)?;
    let selene = load_obj("assets/models/obj/selene.obj").map_err(io::Error::other)?;
    let nymara = load_obj("assets/models/obj/nymara.obj").map_err(io::Error::other)?;
    let ring_inner = load_obj("assets/models/obj/nymara_ring_inner.obj").map_err(io::Error::other)?;
    let ring_middle = load_obj("assets/models/obj/nymara_ring_middle.obj").map_err(io::Error::other)?;
    let ring_outer = load_obj("assets/models/obj/nymara_ring_outer.obj").map_err(io::Error::other)?;
    let cryon = load_obj("assets/models/obj/cryon.obj").map_err(io::Error::other)?;
    let veridis = load_obj("assets/models/obj/veridis.obj").map_err(io::Error::other)?;
    let umbra = load_obj("assets/models/obj/umbra.obj").map_err(io::Error::other)?;
    let solene = load_obj("assets/models/obj/solene.obj").map_err(io::Error::other)?;

    let camera = Camera::new(
        Vec3::new(0.0, 4.8, 11.8),
        Vec3::new(0.0, 0.0, -2.0),
        Vec3::Y,
        0.72,
        800.0 / 600.0,
        0.1,
        50.0,
    )
    .map_err(io::Error::other)?;

    let solaris_motion = BodyMotion {
        orbit: Orbit { center: Vec3::ZERO, radius: 0.0, angular_velocity: 0.0, phase: 0.0, inclination: 0.0 },
        base_rotation: Mat4::identity(), scale: 0.70, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.05, rotation_phase: 0.0,
    };
    let vesper_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -1.35, 0.0), 3.826878624, -2.836369520, 0.10),
        base_rotation: Mat4::rotation_y(-0.25), scale: 0.52, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.22, rotation_phase: 0.0,
    };
    let nymara_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 1.15, 0.0), 3.785828839, -0.588002604, 0.065),
        base_rotation: Mat4::rotation_x(0.48) * Mat4::rotation_z(-0.18), scale: 0.60,
        rotation_axis: RotationAxis::Y, rotation_speed: 0.12, rotation_phase: 0.0,
    };
    let cryon_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 2.75, 0.0), 3.100403200, -1.554668693, 0.045),
        base_rotation: Mat4::rotation_y(0.35), scale: 0.46, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.15, rotation_phase: 0.0,
    };
    let veridis_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -2.65, 0.0), 3.782195130, -1.993047480, 0.055),
        base_rotation: Mat4::rotation_z(0.30), scale: 0.43, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.10, rotation_phase: 0.0,
    };
    let umbra_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -1.75, 0.0), 5.905294235, -0.767436024, 0.035),
        base_rotation: Mat4::rotation_y(-0.40), scale: 0.50, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.08, rotation_phase: 0.0,
    };
    let solene_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 1.65, 0.0), 6.081529413, -2.344567107, 0.028),
        base_rotation: Mat4::rotation_x(-0.22), scale: 0.46, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.07, rotation_phase: 0.0,
    };

    #[cfg(feature = "interactive")]
    if _interactive {
        let time = 0.0;
        let poses = build_poses(
            time, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
            veridis_motion, umbra_motion, solene_motion,
        );
        let mut controller = OrbitCameraController::new(
            BodyId::Solaris, camera.fov_y, camera.aspect, camera.near, camera.far,
        );
        controller.set_orbit(std::f32::consts::FRAC_PI_2, 0.30, 5.0);
        let interactive_camera = controller
            .camera_for_target(poses.get(BodyId::Solaris).world_position)
            .map_err(io::Error::other)?;
        let framebuffer = render_frame(
            time, &interactive_camera, &solaris, &vesper, &selene, &nymara,
            &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra,
            &solene, poses,
        )?;
        let mut window = CpuWindow::new("AETHERIA — CPU Renderer", &framebuffer)
            .map_err(io::Error::other)?;
        window.run_until_closed().map_err(io::Error::other)?;
        return Ok(());
    }

    let outputs = [
        ("output/phase19c_camera_static.ppm", 0.0, CameraMode::Static),
        ("output/phase19c_camera_solaris.ppm", 4.0, CameraMode::Orbit(BodyId::Solaris, 5.0)),
        ("output/phase19c_camera_vesper.ppm", 4.0, CameraMode::Orbit(BodyId::Vesper, 4.8)),
        ("output/phase19c_camera_selene.ppm", 4.0, CameraMode::Orbit(BodyId::Selene, 2.0)),
        ("output/phase19c_camera_nymara.ppm", 4.0, CameraMode::Orbit(BodyId::Nymara, 6.2)),
    ];
    for (output, time, mode) in outputs {
        let poses = build_poses(
            time, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
            veridis_motion, umbra_motion, solene_motion,
        );
        let frame_camera = match mode {
            CameraMode::Static => camera,
            CameraMode::Orbit(target_id, distance) => {
                let mut controller = OrbitCameraController::new(
                    target_id, camera.fov_y, camera.aspect, camera.near, camera.far,
                );
                controller.set_orbit(std::f32::consts::FRAC_PI_2, 0.30, distance);
                controller.camera_for_target(poses.get(target_id).world_position)
                    .map_err(io::Error::other)?
            }
        };
        let framebuffer = render_frame(
            time, &frame_camera, &solaris, &vesper, &selene, &nymara,
            &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
            poses,
        )?;
        fs::create_dir_all("output")?;
        framebuffer.save_ppm(output)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CameraMode {
    Static,
    Orbit(BodyId, f32),
}

fn orbit(center: Vec3, radius: f32, phase: f32, angular_velocity: f32) -> Orbit {
    Orbit { center, radius, angular_velocity, phase, inclination: 0.0 }
}

fn build_poses(
    time: f32,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
) -> BodyPoses {
    let solaris_model = solaris_motion.model_matrix_at(time);
    let vesper_model = vesper_motion.model_matrix_at(time);
    let vesper_position = vesper_motion.orbit.position_at(time);
    let moon_orbit = Orbit {
        center: vesper_position + Vec3::new(0.0, 1.20, 0.0),
        radius: 0.585234996,
        angular_velocity: 0.32,
        phase: 2.792821650,
        inclination: 0.0,
    };
    let selene_position = moon_orbit.position_at(time);
    let selene_model = Mat4::translation(selene_position)
        * Mat4::rotation_z(0.25)
        * Mat4::rotation_y(0.45 * time)
        * Mat4::scale(Vec3::new(0.28, 0.28, 0.28));
    let models = [
        (BodyId::Solaris, solaris_model),
        (BodyId::Vesper, vesper_model),
        (BodyId::Selene, selene_model),
        (BodyId::Nymara, nymara_motion.model_matrix_at(time)),
        (BodyId::Cryon, cryon_motion.model_matrix_at(time)),
        (BodyId::Veridis, veridis_motion.model_matrix_at(time)),
        (BodyId::Umbra, umbra_motion.model_matrix_at(time)),
        (BodyId::Solene, solene_motion.model_matrix_at(time)),
    ];
    BodyPoses::new(models.map(|(id, model_matrix)| BodyPose {
        id,
        world_position: model_matrix.transform_point(Vec3::ZERO),
        model_matrix,
    }))
}

#[allow(clippy::too_many_arguments)]
fn render_frame(
    time: f32, camera: &Camera,
    solaris: &aetheria::assets::ObjAsset, vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset, nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset, ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset, cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset, umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset, poses: BodyPoses,
) -> io::Result<Framebuffer> {
    let solaris_model = poses.get(BodyId::Solaris).model_matrix;
    let vesper_model = poses.get(BodyId::Vesper).model_matrix;
    let selene_model = poses.get(BodyId::Selene).model_matrix;
    let nymara_model = poses.get(BodyId::Nymara).model_matrix;
    let cryon_model = poses.get(BodyId::Cryon).model_matrix;
    let veridis_model = poses.get(BodyId::Veridis).model_matrix;
    let umbra_model = poses.get(BodyId::Umbra).model_matrix;
    let solene_model = poses.get(BodyId::Solene).model_matrix;

    let shadow_scene = Arc::new(ShadowScene::from_meshes(&[
        (1, &vesper.mesh, vesper_model), (2, &selene.mesh, selene_model),
        (3, &nymara.mesh, nymara_model), (4, &ring_inner.mesh, nymara_model),
        (5, &ring_middle.mesh, nymara_model), (6, &ring_outer.mesh, nymara_model),
        (7, &cryon.mesh, cryon_model), (8, &veridis.mesh, veridis_model),
        (9, &umbra.mesh, umbra_model), (10, &solene.mesh, solene_model),
    ]));
    let uniforms = ShaderUniforms {
        time, camera_position: camera.position,
        light_position: solaris_model.transform_point(Vec3::ZERO),
        light_color: Color::new_const(1.0, 0.86, 0.68), light_intensity: 1.8,
        light_attenuation: 0.04, ambient_light: 0.10,
        shadow_scene: Some(shadow_scene.clone()), ..ShaderUniforms::default()
    };
    let mut framebuffer = Framebuffer::new(800, 600).map_err(io::Error::other)?;
    framebuffer.clear(Color::new(0.003, 0.004, 0.010));

    let solaris_shader = SolarisShader::default();
    let vesper_shader = VesperShader::default();
    let selene_shader = SeleneShader::default();
    let nymara_shader = NymaraShader::default();
    let cryon_shader = CryonShader::default();
    let veridis_shader = VeridisShader::default();
    let umbra_shader = UmbraShader::default();
    let solene_shader = SoleneShader::default();
    let ring_inner_shader = NymaraRingShader::new(Color::new_const(0.32, 0.58, 0.62), 0.92, 11);
    let ring_middle_shader = NymaraRingShader::new(Color::new_const(0.50, 0.38, 0.62), 0.78, 23);
    let ring_outer_shader = NymaraRingShader::new(Color::new_const(0.26, 0.42, 0.54), 0.66, 37);

    let render_start = Instant::now();
    let mut render = |receiver: u32, mesh: &aetheria::geometry::Mesh, model: Mat4,
                      shader: &dyn RenderableShader| {
        let mut object_uniforms = uniforms.clone();
        object_uniforms.shadow_receiver = receiver;
        shader.render(&mut framebuffer, mesh, model, camera, &object_uniforms);
    };
    render(u32::MAX, &solaris.mesh, solaris_model, &solaris_shader);
    render(1, &vesper.mesh, vesper_model, &vesper_shader);
    render(2, &selene.mesh, selene_model, &selene_shader);
    render(3, &nymara.mesh, nymara_model, &nymara_shader);
    render(4, &ring_inner.mesh, nymara_model, &ring_inner_shader);
    render(5, &ring_middle.mesh, nymara_model, &ring_middle_shader);
    render(6, &ring_outer.mesh, nymara_model, &ring_outer_shader);
    render(7, &cryon.mesh, cryon_model, &cryon_shader);
    render(8, &veridis.mesh, veridis_model, &veridis_shader);
    render(9, &umbra.mesh, umbra_model, &umbra_shader);
    render(10, &solene.mesh, solene_model, &solene_shader);

    println!("Rendered t={time:.2}; {} shadow triangles; {:?}", shadow_scene.triangle_count(), render_start.elapsed());
    Ok(framebuffer)
}

trait RenderableShader {
    fn render(&self, framebuffer: &mut Framebuffer, mesh: &aetheria::geometry::Mesh,
              model: Mat4, camera: &Camera, uniforms: &ShaderUniforms);
}

impl<S: aetheria::shader::FragmentShader> RenderableShader for S {
    fn render(&self, framebuffer: &mut Framebuffer, mesh: &aetheria::geometry::Mesh,
              model: Mat4, camera: &Camera, uniforms: &ShaderUniforms) {
        render_mesh_with_shader(framebuffer, mesh, model, camera, self, uniforms);
    }
}