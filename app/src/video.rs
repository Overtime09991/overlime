//! Videodarstellung auf der Grafikkarte: NV12 -> RGB (BT.709), Hochskalieren (bikubisch) und
//! Schärfen (CAS, wie bei AMD FidelityFX). Optional (experimentell): Zwischenbilder durch
//! Bewegungsabschätzung, damit 60 FPS auf einem 120-Hz-Bildschirm flüssiger wirken.
//! Die CPU muss kein Bild mehr umrechnen.
use eframe::egui;
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct FrameData {
    pub w: usize,
    pub h: usize,
    pub nv12: Vec<u8>,
    pub dirty: bool,
    pub sharp: f32,
    pub nearest: bool,
    /// Zwischenbilder berechnen (experimentell)
    pub interp: bool,
    /// nur für Tests: feste Position zwischen zwei Bildern (0..1)
    pub force_t: Option<f32>,
}

struct Interp {
    fbo: glow::Framebuffer,
    mv_tex: glow::Texture,
    rgb_tex: glow::Texture,
    mvw: i32,
    mvh: i32,
    w: i32,
    h: i32,
}

struct GlState {
    prog: glow::Program,
    prog_rgb: glow::Program,
    prog_me: glow::Program,
    prog_ip: glow::Program,
    vao: glow::VertexArray,
    ty: [glow::Texture; 2],
    tuv: [glow::Texture; 2],
    cur: usize,
    w: usize,
    h: usize,
    have_prev: bool,
    interp: Option<Interp>,
    interp_failed: bool,
    arrival: Instant,
    interval: f32,
}

pub struct VideoGpu {
    pub frame: Mutex<FrameData>,
    gl: Mutex<Option<GlState>>,
    failed: AtomicBool,
    pub error: Mutex<String>,
}

const FRAG: &str = r#"
uniform vec2 u_out;
uniform float u_sharp;
uniform vec2 u_src;
uniform float u_cubic;
in vec2 v_uv;
out vec4 o_color;

#ifdef RGB_SRC
uniform sampler2D u_rgb;

vec3 yuv(vec2 uv) {
    return texture(u_rgb, uv).rgb;
}

// Catmull-Rom (bikubisch) auf RGB
vec3 yuv_cubic(vec2 uv) {
    vec2 pos = uv * u_src;
    vec2 c = floor(pos - 0.5) + 0.5;
    vec2 f = pos - c;
    vec2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    vec2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    vec2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    vec2 w3 = f * f * (-0.5 + 0.5 * f);
    vec2 w12 = w1 + w2;
    vec2 o12 = w2 / w12;
    vec2 t0 = (c - 1.0) / u_src;
    vec2 t3 = (c + 2.0) / u_src;
    vec2 t12 = (c + o12) / u_src;
    vec3 r = vec3(0.0);
    r += texture(u_rgb, vec2(t0.x, t0.y)).rgb * w0.x * w0.y;
    r += texture(u_rgb, vec2(t12.x, t0.y)).rgb * w12.x * w0.y;
    r += texture(u_rgb, vec2(t3.x, t0.y)).rgb * w3.x * w0.y;
    r += texture(u_rgb, vec2(t0.x, t12.y)).rgb * w0.x * w12.y;
    r += texture(u_rgb, vec2(t12.x, t12.y)).rgb * w12.x * w12.y;
    r += texture(u_rgb, vec2(t3.x, t12.y)).rgb * w3.x * w12.y;
    r += texture(u_rgb, vec2(t0.x, t3.y)).rgb * w0.x * w3.y;
    r += texture(u_rgb, vec2(t12.x, t3.y)).rgb * w12.x * w3.y;
    r += texture(u_rgb, vec2(t3.x, t3.y)).rgb * w3.x * w3.y;
    return clamp(r, 0.0, 1.0);
}
#else
uniform sampler2D u_y;
uniform sampler2D u_uv;

vec3 yuv(vec2 uv) {
    float y = texture(u_y, uv).r;
    vec2 c = texture(u_uv, uv).rg;
    y = (y - 16.0 / 255.0) * (255.0 / 219.0);
    c = (c - 128.0 / 255.0) * (255.0 / 224.0);
    return clamp(vec3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x), 0.0, 1.0);
}

// Catmull-Rom (bikubisch) für die Helligkeit: deutlich schärfere Kanten beim Hochskalieren als bilinear.
float luma_cubic(vec2 uv) {
    vec2 pos = uv * u_src;
    vec2 c = floor(pos - 0.5) + 0.5;
    vec2 f = pos - c;
    vec2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    vec2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    vec2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    vec2 w3 = f * f * (-0.5 + 0.5 * f);
    vec2 w12 = w1 + w2;
    vec2 o12 = w2 / w12;
    vec2 t0 = (c - 1.0) / u_src;
    vec2 t3 = (c + 2.0) / u_src;
    vec2 t12 = (c + o12) / u_src;
    float r = 0.0;
    r += texture(u_y, vec2(t0.x, t0.y)).r * w0.x * w0.y;
    r += texture(u_y, vec2(t12.x, t0.y)).r * w12.x * w0.y;
    r += texture(u_y, vec2(t3.x, t0.y)).r * w3.x * w0.y;
    r += texture(u_y, vec2(t0.x, t12.y)).r * w0.x * w12.y;
    r += texture(u_y, vec2(t12.x, t12.y)).r * w12.x * w12.y;
    r += texture(u_y, vec2(t3.x, t12.y)).r * w3.x * w12.y;
    r += texture(u_y, vec2(t0.x, t3.y)).r * w0.x * w3.y;
    r += texture(u_y, vec2(t12.x, t3.y)).r * w12.x * w3.y;
    r += texture(u_y, vec2(t3.x, t3.y)).r * w3.x * w3.y;
    return clamp(r, 0.0, 1.0);
}

vec3 yuv_cubic(vec2 uv) {
    float y = luma_cubic(uv);
    vec2 c = texture(u_uv, uv).rg;
    y = (y - 16.0 / 255.0) * (255.0 / 219.0);
    c = (c - 128.0 / 255.0) * (255.0 / 224.0);
    return clamp(vec3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x), 0.0, 1.0);
}
#endif

void main() {
    vec2 d = 1.0 / u_out;
    vec3 e = u_cubic > 0.5 ? yuv_cubic(v_uv) : yuv(v_uv);
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

// Bewegungsabschätzung: je 8x8-Block der beste Bewegungsvektor (Vorbild -> aktuelles Bild).
const FRAG_ME: &str = r#"
uniform sampler2D cur_y;
uniform sampler2D prev_y;
uniform vec2 u_src;
in vec2 v_uv;
out vec4 o_color;

float sad(vec2 center, vec2 mv) {
    float s = 0.0;
    for (int j = 0; j < 4; j++) {
        for (int i = 0; i < 4; i++) {
            vec2 o = vec2(float(i) * 5.0 - 7.5, float(j) * 5.0 - 7.5);
            float a = texture(cur_y, (center + o) / u_src).r;
            float b = texture(prev_y, (center + o - mv) / u_src).r;
            s += abs(a - b);
        }
    }
    return s;
}

void main() {
    vec2 center = v_uv * u_src;
    float best = sad(center, vec2(0.0));
    vec2 bmv = vec2(0.0);
    // grob: -24..24 in Schritten von 4 Pixeln
    for (int cy = -24; cy <= 24; cy += 4) {
        for (int cx = -24; cx <= 24; cx += 4) {
            vec2 mv = vec2(float(cx), float(cy));
            float s = sad(center, mv) + 0.002 * (abs(mv.x) + abs(mv.y));
            if (s < best) {
                best = s;
                bmv = mv;
            }
        }
    }
    // fein: +-3 Pixel um den besten Wert
    vec2 base = bmv;
    for (int cy = -3; cy <= 3; cy++) {
        for (int cx = -3; cx <= 3; cx++) {
            vec2 mv = base + vec2(float(cx), float(cy));
            float s = sad(center, mv) + 0.002 * (abs(mv.x) + abs(mv.y));
            if (s < best) {
                best = s;
                bmv = mv;
            }
        }
    }
    o_color = vec4(bmv / 128.0 + 0.5, 0.0, 1.0);
}
"#;

// Zwischenbild an Position u_t (0 = voriges, 1 = aktuelles Bild), mit Rückfall auf einfaches Überblenden.
const FRAG_IP: &str = r#"
uniform sampler2D cur_y;
uniform sampler2D cur_uv;
uniform sampler2D prev_y;
uniform sampler2D prev_uv;
uniform sampler2D mvtex;
uniform vec2 u_src;
uniform float u_t;
in vec2 v_uv;
out vec4 o_color;

vec3 conv(float y, vec2 c) {
    y = (y - 16.0 / 255.0) * (255.0 / 219.0);
    c = (c - 128.0 / 255.0) * (255.0 / 224.0);
    return clamp(vec3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x), 0.0, 1.0);
}
vec3 samp_cur(vec2 uv) { return conv(texture(cur_y, uv).r, texture(cur_uv, uv).rg); }
vec3 samp_prev(vec2 uv) { return conv(texture(prev_y, uv).r, texture(prev_uv, uv).rg); }

void main() {
    vec2 mv = (texture(mvtex, v_uv).rg - 0.5) * 128.0;
    vec3 a = samp_prev(v_uv - mv * u_t / u_src);
    vec3 c = samp_cur(v_uv + mv * (1.0 - u_t) / u_src);
    vec3 mc = mix(a, c, u_t);
    vec3 plain = mix(samp_prev(v_uv), samp_cur(v_uv), u_t);
    float k = smoothstep(0.10, 0.28, length(a - c));
    o_color = vec4(mix(mc, plain, k), 1.0);
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

// Beim Zeichnen in eine Textur steht Zeile 0 unten: hier nicht spiegeln, damit die Zeilenfolge gleich bleibt.
const VERT_FBO: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

impl VideoGpu {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            frame: Mutex::new(FrameData { w: 0, h: 0, nv12: Vec::new(), dirty: false, sharp: 0.0, nearest: false, interp: false, force_t: None }),
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
        let mut new_frame = false;
        unsafe {
            let cw = (f.w + 1) / 2;
            let ch = (f.h + 1) / 2;
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            if s.w != f.w || s.h != f.h {
                for i in 0..2 {
                    gl.bind_texture(glow::TEXTURE_2D, Some(s.ty[i]));
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R8 as i32, f.w as i32, f.h as i32, 0, glow::RED, glow::UNSIGNED_BYTE, None);
                    gl.bind_texture(glow::TEXTURE_2D, Some(s.tuv[i]));
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RG8 as i32, cw as i32, ch as i32, 0, glow::RG, glow::UNSIGNED_BYTE, None);
                }
                s.w = f.w;
                s.h = f.h;
                s.have_prev = false;
                f.dirty = true;
            }
            if f.dirty {
                let ysz = f.w * f.h;
                let uvsz = cw * 2 * ch;
                if f.nv12.len() >= ysz + uvsz {
                    // Zwei Texturpaare im Wechsel: das neue Bild kommt in das ältere Paar, das bisherige bleibt als „voriges Bild“
                    let first = !s.have_prev;
                    let targets: Vec<usize> = if first { vec![0, 1] } else { vec![1 - s.cur] };
                    for idx in targets {
                        gl.bind_texture(glow::TEXTURE_2D, Some(s.ty[idx]));
                        gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, f.w as i32, f.h as i32, glow::RED, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(&f.nv12[..ysz]));
                        gl.bind_texture(glow::TEXTURE_2D, Some(s.tuv[idx]));
                        gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, cw as i32, ch as i32, glow::RG, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(&f.nv12[ysz..ysz + uvsz]));
                    }
                    if first {
                        s.cur = 0;
                        s.have_prev = true;
                    } else {
                        s.cur = 1 - s.cur;
                    }
                    let now = Instant::now();
                    let dt = now.duration_since(s.arrival).as_secs_f32();
                    if dt > 0.004 && dt < 0.1 {
                        s.interval = s.interval * 0.9 + dt * 0.1;
                    }
                    s.arrival = now;
                    new_frame = true;
                }
                f.dirty = false;
            }
            let prev = 1 - s.cur;
            let filter = if f.nearest { glow::NEAREST } else { glow::LINEAR } as i32;
            for t in [s.ty[0], s.ty[1], s.tuv[0], s.tuv[1]] {
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
            }

            // ---- Zwischenbilder (experimentell) ----
            let mut use_rgb = false;
            if f.interp && !s.interp_failed && !f.nearest {
                match s.ensure_interp(gl, f.w as i32, f.h as i32) {
                    Ok(()) => {
                        let ip = s.interp.as_ref().unwrap();
                        // Zustand merken
                        let prev_fb = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
                        let mut vp = [0i32; 4];
                        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
                        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
                        let blend = gl.is_enabled(glow::BLEND);
                        gl.disable(glow::SCISSOR_TEST);
                        gl.disable(glow::BLEND);
                        gl.bind_vertex_array(Some(s.vao));
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ip.fbo));
                        let bind = |unit: u32, tex: glow::Texture| {
                            gl.active_texture(glow::TEXTURE0 + unit);
                            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                        };
                        // Bewegungsabschätzung nur bei einem neuen Bild
                        if new_frame {
                            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(ip.mv_tex), 0);
                            gl.viewport(0, 0, ip.mvw, ip.mvh);
                            gl.use_program(Some(s.prog_me));
                            bind(0, s.ty[s.cur]);
                            bind(1, s.ty[prev]);
                            gl.uniform_1_i32(gl.get_uniform_location(s.prog_me, "cur_y").as_ref(), 0);
                            gl.uniform_1_i32(gl.get_uniform_location(s.prog_me, "prev_y").as_ref(), 1);
                            gl.uniform_2_f32(gl.get_uniform_location(s.prog_me, "u_src").as_ref(), f.w as f32, f.h as f32);
                            gl.draw_arrays(glow::TRIANGLES, 0, 3);
                        }
                        // Zwischenbild berechnen
                        let t = match f.force_t {
                            Some(t) => t,
                            None => (s.arrival.elapsed().as_secs_f32() / s.interval.max(0.004)).clamp(0.0, 1.0),
                        };
                        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(ip.rgb_tex), 0);
                        gl.viewport(0, 0, ip.w, ip.h);
                        gl.use_program(Some(s.prog_ip));
                        bind(0, s.ty[s.cur]);
                        bind(1, s.tuv[s.cur]);
                        bind(2, s.ty[prev]);
                        bind(3, s.tuv[prev]);
                        bind(4, ip.mv_tex);
                        for (i, n) in ["cur_y", "cur_uv", "prev_y", "prev_uv", "mvtex"].iter().enumerate() {
                            gl.uniform_1_i32(gl.get_uniform_location(s.prog_ip, n).as_ref(), i as i32);
                        }
                        gl.uniform_2_f32(gl.get_uniform_location(s.prog_ip, "u_src").as_ref(), f.w as f32, f.h as f32);
                        gl.uniform_1_f32(gl.get_uniform_location(s.prog_ip, "u_t").as_ref(), t);
                        gl.draw_arrays(glow::TRIANGLES, 0, 3);
                        // Zustand zurück
                        let fb = std::num::NonZeroU32::new(prev_fb as u32).map(glow::NativeFramebuffer);
                        gl.bind_framebuffer(glow::FRAMEBUFFER, fb);
                        gl.viewport(vp[0], vp[1], vp[2], vp[3]);
                        if scissor {
                            gl.enable(glow::SCISSOR_TEST);
                        }
                        if blend {
                            gl.enable(glow::BLEND);
                        }
                        gl.active_texture(glow::TEXTURE0);
                        use_rgb = true;
                    }
                    Err(e) => {
                        s.interp_failed = true;
                        *self.error.lock().unwrap() = e;
                    }
                }
            }

            // ---- Ausgabe ----
            let sharp = if f.nearest { 0.0 } else { f.sharp };
            let cubic = !f.nearest && f.sharp > 0.001 && out_w > f.w as f32 * 1.02;
            gl.bind_vertex_array(Some(s.vao));
            if use_rgb {
                let ip = s.interp.as_ref().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(ip.rgb_tex));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.use_program(Some(s.prog_rgb));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(ip.rgb_tex));
                gl.uniform_1_i32(gl.get_uniform_location(s.prog_rgb, "u_rgb").as_ref(), 0);
                gl.uniform_2_f32(gl.get_uniform_location(s.prog_rgb, "u_out").as_ref(), out_w.max(1.0), out_h.max(1.0));
                gl.uniform_1_f32(gl.get_uniform_location(s.prog_rgb, "u_sharp").as_ref(), sharp);
                gl.uniform_2_f32(gl.get_uniform_location(s.prog_rgb, "u_src").as_ref(), f.w as f32, f.h as f32);
                gl.uniform_1_f32(gl.get_uniform_location(s.prog_rgb, "u_cubic").as_ref(), if cubic { 1.0 } else { 0.0 });
            } else {
                gl.use_program(Some(s.prog));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(s.ty[s.cur]));
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(s.tuv[s.cur]));
                gl.uniform_1_i32(gl.get_uniform_location(s.prog, "u_y").as_ref(), 0);
                gl.uniform_1_i32(gl.get_uniform_location(s.prog, "u_uv").as_ref(), 1);
                gl.uniform_2_f32(gl.get_uniform_location(s.prog, "u_out").as_ref(), out_w.max(1.0), out_h.max(1.0));
                gl.uniform_1_f32(gl.get_uniform_location(s.prog, "u_sharp").as_ref(), sharp);
                gl.uniform_2_f32(gl.get_uniform_location(s.prog, "u_src").as_ref(), f.w as f32, f.h as f32);
                gl.uniform_1_f32(gl.get_uniform_location(s.prog, "u_cubic").as_ref(), if cubic { 1.0 } else { 0.0 });
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_vertex_array(None);
        }
    }
}

impl GlState {
    unsafe fn ensure_interp(&mut self, gl: &glow::Context, w: i32, h: i32) -> Result<(), String> {
        if let Some(ip) = &self.interp {
            if ip.w == w && ip.h == h {
                return Ok(());
            }
            gl.delete_framebuffer(ip.fbo);
            gl.delete_texture(ip.mv_tex);
            gl.delete_texture(ip.rgb_tex);
            self.interp = None;
        }
        let mk = |gl: &glow::Context, tw: i32, th: i32, filter: u32| -> Result<glow::Texture, String> {
            let t = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, tw, th, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            Ok(t)
        };
        let (mvw, mvh) = ((w + 7) / 8, (h + 7) / 8);
        let mv_tex = mk(gl, mvw, mvh, glow::NEAREST)?;
        let rgb_tex = mk(gl, w, h, glow::LINEAR)?;
        let fbo = gl.create_framebuffer()?;
        // Prüfen, ob die Grafikkarte das Zeichnen in diese Texturen erlaubt
        let prev_fb = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(rgb_tex), 0);
        let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        let fb = std::num::NonZeroU32::new(prev_fb as u32).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, fb);
        if !ok {
            return Err("Zwischenbilder: Grafikkarte unterstützt das Zeichnen in Texturen nicht".into());
        }
        self.interp = Some(Interp { fbo, mv_tex, rgb_tex, mvw, mvh, w, h });
        Ok(())
    }

    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let v = gl.version();
        let ver = if v.major > 3 || (v.major == 3 && v.minor >= 3) { "#version 330 core\n" } else { "#version 140\n" };
        let build = |extra: &str, vsrc: &str, fsrc: &str| -> Result<glow::Program, String> {
            let compile = |kind: u32, src: &str| -> Result<glow::Shader, String> {
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &format!("{ver}{extra}{src}"));
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    return Err(format!("Shader: {}", gl.get_shader_info_log(sh)));
                }
                Ok(sh)
            };
            let vs = compile(glow::VERTEX_SHADER, vsrc)?;
            let fs = compile(glow::FRAGMENT_SHADER, fsrc)?;
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
            Ok(program)
        };
        let prog = build("", VERT, FRAG)?;
        // Die Zwischenbild-Teile sind optional: schlagen sie fehl, läuft das Video trotzdem normal.
        let prog_rgb = build("#define RGB_SRC 1\n", VERT, FRAG);
        let prog_me = build("", VERT_FBO, FRAG_ME);
        let prog_ip = build("", VERT_FBO, FRAG_IP);
        let (prog_rgb, prog_me, prog_ip, interp_failed) = match (prog_rgb, prog_me, prog_ip) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c, false),
            _ => (prog, prog, prog, true),
        };
        let vao = gl.create_vertex_array()?;
        let mk = |gl: &glow::Context| -> Result<glow::Texture, String> {
            let t = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            Ok(t)
        };
        Ok(GlState {
            prog,
            prog_rgb,
            prog_me,
            prog_ip,
            vao,
            ty: [mk(gl)?, mk(gl)?],
            tuv: [mk(gl)?, mk(gl)?],
            cur: 0,
            w: 0,
            h: 0,
            have_prev: false,
            interp: None,
            interp_failed,
            arrival: Instant::now(),
            interval: 1.0 / 60.0,
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

/// Verschiebt ein NV12-Bild waagerecht (zyklisch) um dx Pixel (nur für den Zwischenbild-Test).
pub fn shift_nv12(src: &[u8], w: usize, h: usize, dx: usize) -> Vec<u8> {
    let mut out = src.to_vec();
    let dx = dx % w;
    for y in 0..h {
        for x in 0..w {
            out[y * w + (x + dx) % w] = src[y * w + x];
        }
    }
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;
    let base = w * h;
    let dxc = (dx / 2) % cw;
    for y in 0..ch {
        for x in 0..cw {
            let (a, b) = (base + y * cw * 2 + x * 2, base + y * cw * 2 + ((x + dxc) % cw) * 2);
            out[b] = src[a];
            out[b + 1] = src[a + 1];
        }
    }
    out
}


/// Durchschnittsfarbe eines NV12-Bildes (grobes Raster, sehr schnell), Werte 0..1.
pub fn nv12_avg_color(nv12: &[u8], w: usize, h: usize) -> Option<[f32; 3]> {
    if w < 16 || h < 16 || nv12.len() < w * h * 3 / 2 {
        return None;
    }
    let (gx, gy) = (48usize, 27usize);
    let (mut sr, mut sg, mut sb, mut n) = (0f32, 0f32, 0f32, 0f32);
    for j in 0..gy {
        let y = (j * h / gy + h / (2 * gy)).min(h - 1);
        for i in 0..gx {
            let x = (i * w / gx + w / (2 * gx)).min(w - 1);
            let yy = nv12[y * w + x] as f32;
            let uv = w * h + (y / 2) * w + (x / 2) * 2;
            let (u, v) = (nv12[uv] as f32 - 128.0, nv12[uv + 1] as f32 - 128.0);
            let yv = 1.164 * (yy - 16.0);
            sr += (yv + 1.793 * v).clamp(0.0, 255.0);
            sg += (yv - 0.213 * u - 0.533 * v).clamp(0.0, 255.0);
            sb += (yv + 2.112 * u).clamp(0.0, 255.0);
            n += 1.0;
        }
    }
    Some([sr / n / 255.0, sg / n / 255.0, sb / n / 255.0])
}

/// Macht aus der Durchschnittsfarbe eine Farbe für die Lichtleiste: kräftiger, nie ganz dunkel.
pub fn ambilight_color(c: [f32; 3], bright: f32) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let sat = if max > 0.001 { (max - min) / max } else { 0.0 };
    let sat2 = (sat * 1.5 + 0.1).min(1.0);
    // auf volle Helligkeit hochskalieren und dann mit Wunschhelligkeit und Bildhelligkeit mischen
    let lum = (0.2 + 0.8 * max.sqrt()).min(1.0) * bright;
    let base = if max > 0.001 { [c[0] / max, c[1] / max, c[2] / max] } else { [1.0, 1.0, 1.0] };
    let mut out = [0f32; 3];
    for i in 0..3 {
        // Sättigung erhöhen: Abstand zum Grau vergrößern
        let grey = 1.0 - sat;
        let v = grey + (base[i] - grey) * (sat2 / sat.max(0.001)).min(4.0);
        out[i] = (v.clamp(0.0, 1.0)) * lum;
    }
    out
}
