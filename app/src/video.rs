//! Videodarstellung auf der Grafikkarte: NV12 -> RGB (BT.709), Hochskalieren und
//! Schärfen (CAS, wie bei AMD FidelityFX). Die CPU muss kein Bild mehr umrechnen.
use eframe::egui;
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct FrameData {
    pub w: usize,
    pub h: usize,
    pub nv12: Vec<u8>,
    pub dirty: bool,
    pub sharp: f32,
    pub nearest: bool,
}

struct GlState {
    program: glow::Program,
    vao: glow::VertexArray,
    tex_y: glow::Texture,
    tex_uv: glow::Texture,
    w: usize,
    h: usize,
    u_y: Option<glow::UniformLocation>,
    u_uv: Option<glow::UniformLocation>,
    u_out: Option<glow::UniformLocation>,
    u_sharp: Option<glow::UniformLocation>,
}

pub struct VideoGpu {
    pub frame: Mutex<FrameData>,
    gl: Mutex<Option<GlState>>,
    failed: AtomicBool,
    pub error: Mutex<String>,
}

const FRAG: &str = r#"
uniform sampler2D u_y;
uniform sampler2D u_uv;
uniform vec2 u_out;
uniform float u_sharp;
in vec2 v_uv;
out vec4 o_color;

vec3 yuv(vec2 uv) {
    float y = texture(u_y, uv).r;
    vec2 c = texture(u_uv, uv).rg;
    y = (y - 16.0 / 255.0) * (255.0 / 219.0);
    c = (c - 128.0 / 255.0) * (255.0 / 224.0);
    return clamp(vec3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x), 0.0, 1.0);
}

void main() {
    vec2 d = 1.0 / u_out;
    vec3 e = yuv(v_uv);
    if (u_sharp > 0.001) {
        vec3 b = yuv(v_uv + vec2(0.0, -d.y));
        vec3 l = yuv(v_uv + vec2(-d.x, 0.0));
        vec3 r = yuv(v_uv + vec2(d.x, 0.0));
        vec3 h = yuv(v_uv + vec2(0.0, d.y));
        vec3 mn = min(min(min(l, e), min(r, b)), h);
        vec3 mx = max(max(max(l, e), max(r, b)), h);
        vec3 amp = sqrt(clamp(min(mn, 1.0 - mx) / max(mx, vec3(0.001)), 0.0, 1.0));
        float peak = -1.0 / mix(8.0, 5.0, u_sharp);
        vec3 w = amp * peak;
        e = clamp(((b + l + r + h) * w + e) / (1.0 + 4.0 * w), 0.0, 1.0);
    }
    o_color = vec4(e, 1.0);
}
"#;

const VERT: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

impl VideoGpu {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            frame: Mutex::new(FrameData { w: 0, h: 0, nv12: Vec::new(), dirty: false, sharp: 0.0, nearest: false }),
            gl: Mutex::new(None),
            failed: AtomicBool::new(false),
            error: Mutex::new(String::new()),
        })
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    fn fail(&self, msg: String) {
        *self.error.lock().unwrap() = msg;
        self.failed.store(true, Ordering::Relaxed);
    }

    /// Zeichnet das Video in `rect` (läuft über egui als Callback).
    pub fn paint(self: &Arc<Self>, painter: &egui::Painter, rect: egui::Rect) {
        let me = self.clone();
        let cb = egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                let px = info.viewport_in_pixels();
                me.render(painter.gl(), px.width_px as f32, px.height_px as f32);
            })),
        };
        painter.add(cb);
    }

    fn render(&self, gl: &Arc<glow::Context>, out_w: f32, out_h: f32) {
        if self.failed() {
            return;
        }
        let mut st = self.gl.lock().unwrap();
        if st.is_none() {
            match unsafe { GlState::new(gl) } {
                Ok(s) => *st = Some(s),
                Err(e) => {
                    drop(st);
                    self.fail(e);
                    return;
                }
            }
        }
        let s = st.as_mut().unwrap();
        let mut f = self.frame.lock().unwrap();
        if f.w == 0 || f.h == 0 {
            return;
        }
        unsafe {
            let cw = (f.w + 1) / 2;
            let ch = (f.h + 1) / 2;
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            if s.w != f.w || s.h != f.h {
                gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_y));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R8 as i32, f.w as i32, f.h as i32, 0, glow::RED, glow::UNSIGNED_BYTE, None);
                gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_uv));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RG8 as i32, cw as i32, ch as i32, 0, glow::RG, glow::UNSIGNED_BYTE, None);
                s.w = f.w;
                s.h = f.h;
                f.dirty = true;
            }
            if f.dirty {
                let ysz = f.w * f.h;
                let uvsz = cw * 2 * ch;
                if f.nv12.len() >= ysz + uvsz {
                    gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_y));
                    gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, f.w as i32, f.h as i32, glow::RED, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(&f.nv12[..ysz]));
                    gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_uv));
                    gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, cw as i32, ch as i32, glow::RG, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(&f.nv12[ysz..ysz + uvsz]));
                }
                f.dirty = false;
            }
            let filter = if f.nearest { glow::NEAREST } else { glow::LINEAR } as i32;
            for t in [s.tex_y, s.tex_uv] {
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
            }
            gl.use_program(Some(s.program));
            gl.bind_vertex_array(Some(s.vao));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_y));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_uv));
            gl.uniform_1_i32(s.u_y.as_ref(), 0);
            gl.uniform_1_i32(s.u_uv.as_ref(), 1);
            gl.uniform_2_f32(s.u_out.as_ref(), out_w.max(1.0), out_h.max(1.0));
            gl.uniform_1_f32(s.u_sharp.as_ref(), if f.nearest { 0.0 } else { f.sharp });
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_vertex_array(None);
        }
    }
}

impl GlState {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let v = gl.version();
        let header = if v.major > 3 || (v.major == 3 && v.minor >= 3) { "#version 330 core\n" } else { "#version 140\n" };
        let compile = |kind: u32, src: &str| -> Result<glow::Shader, String> {
            let sh = gl.create_shader(kind)?;
            gl.shader_source(sh, &format!("{header}{src}"));
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                return Err(format!("Shader: {}", gl.get_shader_info_log(sh)));
            }
            Ok(sh)
        };
        let vs = compile(glow::VERTEX_SHADER, VERT)?;
        let fs = compile(glow::FRAGMENT_SHADER, FRAG)?;
        let program = gl.create_program()?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(format!("Programm: {}", gl.get_program_info_log(program)));
        }
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        let vao = gl.create_vertex_array()?;
        let mk = |gl: &glow::Context| -> Result<glow::Texture, String> {
            let t = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            Ok(t)
        };
        let tex_y = mk(gl)?;
        let tex_uv = mk(gl)?;
        Ok(GlState {
            program,
            vao,
            tex_y,
            tex_uv,
            w: 0,
            h: 0,
            u_y: gl.get_uniform_location(program, "u_y"),
            u_uv: gl.get_uniform_location(program, "u_uv"),
            u_out: gl.get_uniform_location(program, "u_out"),
            u_sharp: gl.get_uniform_location(program, "u_sharp"),
        })
    }
}

/// NV12 (BT.709, begrenzter Bereich) -> RGBA, für Screenshots.
pub fn nv12_to_rgba(nv12: &[u8], w: usize, h: usize) -> Vec<u8> {
    let cw = (w + 1) / 2;
    let ysz = w * h;
    let mut out = vec![255u8; w * h * 4];
    if nv12.len() < ysz + cw * 2 * ((h + 1) / 2) {
        return out;
    }
    for y in 0..h {
        for x in 0..w {
            let yy = (nv12[y * w + x] as f32 - 16.0) * (255.0 / 219.0);
            let ci = ysz + (y / 2) * cw * 2 + (x / 2) * 2;
            let u = (nv12[ci] as f32 - 128.0) * (255.0 / 224.0);
            let v = (nv12[ci + 1] as f32 - 128.0) * (255.0 / 224.0);
            let r = (yy + 1.5748 * v).clamp(0.0, 255.0);
            let g = (yy - 0.1873 * u - 0.4681 * v).clamp(0.0, 255.0);
            let b = (yy + 1.8556 * u).clamp(0.0, 255.0);
            let o = (y * w + x) * 4;
            out[o] = r as u8;
            out[o + 1] = g as u8;
            out[o + 2] = b as u8;
        }
    }
    out
}

/// Testbild (nur für den Demo-Modus): Farbbalken, Verlauf, feine Linien, Schrift-ähnliche Kanten.
pub fn test_pattern_nv12(w: usize, h: usize) -> Vec<u8> {
    let mut rgb = vec![0u8; w * h * 3];
    let bars: [[u8; 3]; 8] = [[235, 235, 235], [235, 235, 16], [16, 235, 235], [16, 235, 16], [235, 16, 235], [235, 16, 16], [16, 16, 235], [16, 16, 16]];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 3;
            let c: [u8; 3] = if y < h * 2 / 5 {
                bars[(x * 8 / w).min(7)]
            } else if y < h * 3 / 5 {
                let g = (x * 255 / w) as u8;
                [g, g, g]
            } else if y < h * 4 / 5 {
                // feine Linien (1 px) und Schachbrett
                if x < w / 2 {
                    if (x + y) % 2 == 0 { [240, 240, 240] } else { [20, 20, 20] }
                } else if (x / 3 + y / 3) % 2 == 0 {
                    [240, 240, 240]
                } else {
                    [20, 20, 20]
                }
            } else {
                // Kreis und Diagonale (Kanten)
                let (dx, dy) = (x as f32 - w as f32 * 0.25, y as f32 - h as f32 * 0.9);
                let in_circle = (dx * dx + dy * dy).sqrt() < h as f32 * 0.07;
                let diag = ((x as i32 - (y as i32 * 3)) % 40).abs() < 2 && x > w / 2;
                if in_circle || diag { [250, 200, 40] } else { [30, 40, 70] }
            };
            rgb[o] = c[0];
            rgb[o + 1] = c[1];
            rgb[o + 2] = c[2];
        }
    }
    // RGB -> NV12 (BT.709 begrenzt)
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;
    let mut out = vec![0u8; w * h + cw * 2 * ch];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 3;
            let (r, g, b) = (rgb[o] as f32, rgb[o + 1] as f32, rgb[o + 2] as f32);
            out[y * w + x] = (16.0 + (0.2126 * r + 0.7152 * g + 0.0722 * b) * 219.0 / 255.0).clamp(16.0, 235.0) as u8;
        }
    }
    for cy in 0..ch {
        for cx in 0..cw {
            let (mut r, mut g, mut b, mut n) = (0f32, 0f32, 0f32, 0f32);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (x, y) = ((cx * 2 + dx).min(w - 1), (cy * 2 + dy).min(h - 1));
                    let o = (y * w + x) * 3;
                    r += rgb[o] as f32;
                    g += rgb[o + 1] as f32;
                    b += rgb[o + 2] as f32;
                    n += 1.0;
                }
            }
            let (r, g, b) = (r / n, g / n, b / n);
            let yy = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let u = (b - yy) / 1.8556;
            let v = (r - yy) / 1.5748;
            let o = w * h + cy * cw * 2 + cx * 2;
            out[o] = (128.0 + u * 224.0 / 255.0).clamp(16.0, 240.0) as u8;
            out[o + 1] = (128.0 + v * 224.0 / 255.0).clamp(16.0, 240.0) as u8;
        }
    }
    out
}
