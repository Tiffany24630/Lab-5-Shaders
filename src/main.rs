use std::fs;
use std::io;

use aetheria::assets::load_obj;
use aetheria::camera::Camera;
use aetheria::color::Color;
use aetheria::framebuffer::Framebuffer;
use aetheria::math::{Mat4, Vec3};
use aetheria::pipeline::render_mesh_with_shader;
use aetheria::shader::ShaderUniforms;
use aetheria::shaders::selene::SeleneShader;

fn main() -> io::Result<()> {
    let selene = load_obj("assets/models/obj/selene.obj").map_err(io::Error::other)?;

    let mut framebuffer = Framebuffer::new(800, 600).map_err(io::Error::other)?;
    framebuffer.clear(Color::new(0.006, 0.008, 0.012));
    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 5.5),
        Vec3::ZERO,
        Vec3::Y,
        0.8,
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
    let shader = SeleneShader::default();

    render_mesh_with_shader(
        &mut framebuffer,
        &selene.mesh,
        Mat4::scale(Vec3::new(1.55, 1.55, 1.55)),
        &camera,
        &shader,
        &uniforms,
    );
    fs::create_dir_all("output")?;
    framebuffer.save_ppm("output/phase14_selene.ppm")?;
    println!("Loaded Selene OBJ and rendered it with SeleneShader");
    println!("Wrote output/phase14_selene.ppm (800x600)");
    Ok(())
}