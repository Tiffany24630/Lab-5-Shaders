use std::fs;
use std::io;

use aetheria::assets::load_obj;
use aetheria::camera::Camera;
use aetheria::color::Color;
use aetheria::framebuffer::Framebuffer;
use aetheria::math::{Mat4, Vec3};
use aetheria::pipeline::render_mesh_with_shader;
use aetheria::shader::ShaderUniforms;
use aetheria::shaders::nymara::{NymaraRingShader, NymaraShader};

fn main() -> io::Result<()> {
    let nymara = load_obj("assets/models/obj/nymara.obj").map_err(io::Error::other)?;
    let ring_inner =
        load_obj("assets/models/obj/nymara_ring_inner.obj").map_err(io::Error::other)?;
    let ring_middle =
        load_obj("assets/models/obj/nymara_ring_middle.obj").map_err(io::Error::other)?;
    let ring_outer =
        load_obj("assets/models/obj/nymara_ring_outer.obj").map_err(io::Error::other)?;

    let mut framebuffer = Framebuffer::new(800, 600).map_err(io::Error::other)?;
    framebuffer.clear(Color::new(0.006, 0.010, 0.018));
    let camera = Camera::new(
        Vec3::new(0.0, 1.0, 8.0),
        Vec3::new(0.0, 0.0, -1.5),
        Vec3::Y,
        1.0,
        800.0 / 600.0,
        0.1,
        40.0,
    )
    .map_err(io::Error::other)?;
    let uniforms = ShaderUniforms {
        time: 0.0,
        camera_position: camera.position,
        light_position: Vec3::new(-4.0, 4.0, 5.0),
        ..ShaderUniforms::default()
    };
    let nymara_shader = NymaraShader::default();
    let nymara_model = Mat4::translation(Vec3::new(0.0, 0.0, -2.8))
        * Mat4::rotation_x(0.35)
        * Mat4::scale(Vec3::new(0.9, 0.9, 0.9));

    render_mesh_with_shader(
        &mut framebuffer,
        &nymara.mesh,
        nymara_model,
        &camera,
        &nymara_shader,
        &uniforms,
    );
    let ring_inner_shader = NymaraRingShader::new(Color::new_const(0.32, 0.58, 0.62), 0.92, 11);
    let ring_middle_shader = NymaraRingShader::new(Color::new_const(0.50, 0.38, 0.62), 0.78, 23);
    let ring_outer_shader = NymaraRingShader::new(Color::new_const(0.26, 0.42, 0.54), 0.66, 37);
    render_mesh_with_shader(
        &mut framebuffer,
        &ring_inner.mesh,
        nymara_model,
        &camera,
        &ring_inner_shader,
        &uniforms,
    );
    render_mesh_with_shader(
        &mut framebuffer,
        &ring_middle.mesh,
        nymara_model,
        &camera,
        &ring_middle_shader,
        &uniforms,
    );
    render_mesh_with_shader(
        &mut framebuffer,
        &ring_outer.mesh,
        nymara_model,
        &camera,
        &ring_outer_shader,
        &uniforms,
    );
    fs::create_dir_all("output")?;
    framebuffer.save_ppm("output/phase9_nymara.ppm")?;
    println!("Loaded Nymara OBJ and three independent ring OBJ files");
    println!("Wrote output/phase9_nymara.ppm (800x600)");
    Ok(())
}