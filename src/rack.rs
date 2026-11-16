//! Involute rack geometry (the straight-flanked limit of an involute gear).
//!
//! Everything is in millimetres. The rack lies along +X on the XY plane with its pitch line on
//! y = 0 and its teeth pointing towards +Y, so a gear made by `make-gear` meshes with it when
//! its centre is at (x, m z / 2). All edges are straight, so the whole outline is one closed
//! polyline that is extruded once (no pattern, so many teeth stay cheap).
//!
//! Definitions used:
//! * circular pitch `p = pi m`
//! * addendum `m`, dedendum `1.25 m`, flanks inclined `a` (the pressure angle) from the tooth axis
//! * half tooth width at height `y` above the pitch line: `p/4 - y tan(a)`

use std::f64::consts::PI;

pub type P = [f64; 2];

#[derive(Clone, Copy, Debug)]
pub struct RackParams {
    pub module: f64,
    pub teeth: u32,
    pub pressure_angle_deg: f64,
    pub face_width: f64,
    /// Material below the root line.
    pub body_height: f64,
}

#[derive(Clone, Debug)]
pub struct RackDims {
    pub pitch: f64,
    pub length: f64,
    pub tip_y: f64,
    pub root_y: f64,
    pub bottom_y: f64,
    pub tip_width: f64,
    pub root_land: f64,
}

/// Most teeth one rack may have: every edge is one host command, and the host has a time limit.
pub const MAX_TEETH: u32 = 100;

pub fn dims(p: &RackParams) -> RackDims {
    let m = p.module;
    let t = p.pressure_angle_deg.to_radians().tan();
    let pitch = PI * m;
    RackDims {
        pitch,
        length: pitch * p.teeth as f64,
        tip_y: m,
        root_y: -1.25 * m,
        bottom_y: -1.25 * m - p.body_height,
        tip_width: pitch / 2.0 - 2.0 * m * t,
        root_land: pitch / 2.0 - 2.5 * m * t,
    }
}

pub fn validate(p: &RackParams) -> Result<(), (String, String)> {
    let bad = |f: &str, r: &str| Err((f.to_string(), r.to_string()));
    if !(p.module.is_finite() && p.module >= 0.1 && p.module <= 100.0) {
        return bad("module", "must be between 0.1 and 100 mm");
    }
    if !(1..=MAX_TEETH).contains(&p.teeth) {
        return bad("teeth", "must be between 1 and 100");
    }
    if !(p.pressure_angle_deg.is_finite() && (10.0..=30.0).contains(&p.pressure_angle_deg)) {
        return bad("pressure_angle", "must be between 10 and 30 degrees");
    }
    if !(p.face_width.is_finite() && p.face_width > 0.0) {
        return bad("face_width", "must be positive");
    }
    if !(p.body_height.is_finite() && p.body_height > 0.0) {
        return bad("body_height", "must be positive");
    }
    let d = dims(p);
    if d.tip_width <= 0.0 || d.root_land <= 0.0 {
        return bad("pressure_angle", "teeth would come to a point or overlap");
    }
    Ok(())
}

/// The closed outline, counter-clockwise, as vertices (the last one joins back to the first).
/// Tooth i is centred on x = p (i + 1/2); the rack spans x = 0 .. teeth p.
pub fn outline(p: &RackParams) -> Vec<P> {
    let m = p.module;
    let d = dims(p);
    let t = p.pressure_angle_deg.to_radians().tan();
    let half_root = d.pitch / 4.0 + 1.25 * m * t;
    let half_tip = d.pitch / 4.0 - m * t;
    let mut v: Vec<P> = vec![[0.0, d.root_y]];
    for i in 0..p.teeth {
        let xc = d.pitch * (i as f64 + 0.5);
        v.push([xc - half_root, d.root_y]);
        v.push([xc - half_tip, d.tip_y]);
        v.push([xc + half_tip, d.tip_y]);
        v.push([xc + half_root, d.root_y]);
    }
    v.push([d.length, d.root_y]);
    v.push([d.length, d.bottom_y]);
    v.push([0.0, d.bottom_y]);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(z: u32) -> RackParams {
        RackParams { module: 2.0, teeth: z, pressure_angle_deg: 20.0, face_width: 10.0, body_height: 6.0 }
    }
    fn dist(a: P, b: P) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }
    #[test]
    fn no_degenerate_edges() {
        for z in [1, 2, 10, 100] {
            for a in [10.0, 20.0, 25.0, 30.0] {
                let mut q = p(z);
                q.pressure_angle_deg = a;
                validate(&q).unwrap();
                let v = outline(&q);
                for i in 0..v.len() {
                    assert!(dist(v[i], v[(i + 1) % v.len()]) > 1e-6, "z{z} a{a} edge {i}");
                }
            }
        }
    }
    #[test]
    fn tooth_thickness_at_pitch_line() {
        // width of the tooth where the flanks cross y = 0 must be pi m / 2
        let q = p(3);
        let v = outline(&q);
        // tooth 1 vertices: indices 1+4..1+8 -> root_l, tip_l, tip_r, root_r
        let (rl, tl, tr, rr) = (v[5], v[6], v[7], v[8]);
        let cross = |a: P, b: P| a[0] + (b[0] - a[0]) * (0.0 - a[1]) / (b[1] - a[1]);
        let w = cross(rr, tr) - cross(rl, tl);
        assert!((w - PI * 2.0 / 2.0).abs() < 1e-9, "{w}");
    }
    #[test]
    fn flank_angle_matches_pressure_angle() {
        let q = p(2);
        let v = outline(&q);
        let (rl, tl) = (v[1], v[2]);
        let ang = ((tl[0] - rl[0]) / (tl[1] - rl[1])).atan().to_degrees().abs();
        assert!((ang - 20.0).abs() < 1e-9, "{ang}");
    }
    #[test]
    fn dimensions() {
        let d = dims(&p(10));
        assert!((d.length - 10.0 * PI * 2.0).abs() < 1e-9);
        assert!((d.tip_y - 2.0).abs() < 1e-12 && (d.root_y + 2.5).abs() < 1e-12);
        assert!((d.bottom_y + 8.5).abs() < 1e-12);
    }
    #[test]
    fn rejects_bad_input() {
        let mut q = p(5);
        q.teeth = 0;
        assert!(validate(&q).is_err());
        q.teeth = 101;
        assert!(validate(&q).is_err());
        q = p(5);
        q.body_height = 0.0;
        assert!(validate(&q).is_err());
    }
}
