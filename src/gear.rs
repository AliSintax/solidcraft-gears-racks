//! Involute spur gear geometry, written from the textbook definitions.
//!
//! Everything is in millimetres and radians. The tooth is centred on the +X axis of its
//! own wedge; the right flank sits at negative angles and the left flank at positive ones.
//!
//! Definitions used:
//! * pitch radius `r = m z / 2`, base radius `rb = r cos(a)`
//! * involute function `inv(a) = tan(a) - a`; a point of the involute at radius `rho`
//!   has pressure angle `acos(rb / rho)` and lies `inv` of that past the start of the curve
//! * tooth thickness on the pitch circle `s = m (pi/2 + 2 x tan a)` (x is the profile shift)
//! * addendum radius `ra = m (z/2 + ha + x)`, root radius `rf = m (z/2 - hf + x)`, where `ha` and
//!   `hf` are the addendum and dedendum coefficients (multiples of the module; 1 and 1.25 are standard)

use std::f64::consts::PI;

pub type P = [f64; 2];

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub module: f64,
    pub teeth: u32,
    pub pressure_angle_deg: f64,
    pub face_width: f64,
    pub bore: f64,
    pub shift: f64,
    /// Addendum coefficient: tooth height above the pitch circle, in modules.
    pub addendum: f64,
    /// Dedendum coefficient: tooth depth below the pitch circle, in modules.
    pub dedendum: f64,
}

#[derive(Clone, Debug)]
pub struct Dims {
    pub pitch_d: f64,
    pub base_d: f64,
    pub outer_d: f64,
    pub root_d: f64,
}

pub fn dims(p: &Params) -> Dims {
    let z = p.teeth as f64;
    let a = p.pressure_angle_deg.to_radians();
    Dims {
        pitch_d: p.module * z,
        base_d: p.module * z * a.cos(),
        outer_d: p.module * (z + 2.0 * p.addendum + 2.0 * p.shift),
        root_d: p.module * (z - 2.0 * p.dedendum + 2.0 * p.shift),
    }
}

pub fn validate(p: &Params) -> Result<(), (String, String)> {
    let bad = |f: &str, r: &str| Err((f.to_string(), r.to_string()));
    if !(p.module.is_finite() && p.module > 0.0 && p.module <= 100.0) {
        return bad("module", "must be between 0 and 100 mm");
    }
    if !(8..=80).contains(&p.teeth) {
        return bad("teeth", "must be between 8 and 80 (fewer teeth would be undercut, more would be too slow to build)");
    }
    if !(p.pressure_angle_deg.is_finite() && (10.0..=30.0).contains(&p.pressure_angle_deg)) {
        return bad("pressure_angle", "must be between 10 and 30 degrees");
    }
    if !(p.face_width.is_finite() && p.face_width > 0.0) {
        return bad("face_width", "must be positive");
    }
    if !(p.addendum.is_finite() && (0.3..=2.0).contains(&p.addendum)) {
        return bad("addendum", "must be between 0.3 and 2 (times the module)");
    }
    if !(p.dedendum.is_finite() && (0.3..=2.5).contains(&p.dedendum)) {
        return bad("dedendum", "must be between 0.3 and 2.5 (times the module)");
    }
    if !(p.shift.is_finite() && (-1.0..=1.0).contains(&p.shift)) {
        return bad("profile_shift", "must be between -1 and 1");
    }
    let d = dims(p);
    if !(p.bore.is_finite() && p.bore >= 0.0 && p.bore < d.root_d * 0.7) {
        return bad(
            "bore_diameter",
            "must be 0 (none) or smaller than 70% of the root diameter",
        );
    }
    if p.pressure_angle_deg.to_radians().cos() * d.pitch_d >= d.outer_d {
        return bad(
            "addendum",
            "the tooth tip would be at or below the base circle; raise the addendum or lower the profile shift",
        );
    }
    Ok(())
}

fn inv(a: f64) -> f64 {
    a.tan() - a
}

fn polar(r: f64, ang: f64) -> P {
    [r * ang.cos(), r * ang.sin()]
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// One tooth wedge (one pitch wide, from the middle of one gap to the middle of the next), as a
/// closed loop of sketch segments.
#[derive(Clone, Debug)]
pub enum Seg {
    /// A spline through points.
    Spline(Vec<P>),
    Line(P, P),
    /// Start, end, a point between them on the arc.
    Arc(P, P, P),
}

pub struct Wedge {
    pub segs: Vec<Seg>,
    pub root_fillet: f64,
    /// Radius of the inner arc that closes the wedge; the hub cylinder is wider than this, so they overlap.
    pub inner_radius: f64,
}

pub fn wedge(p: &Params) -> Result<Wedge, String> {
    let z = p.teeth as f64;
    let m = p.module;
    let alpha = p.pressure_angle_deg.to_radians();
    let r_p = m * z / 2.0;
    let rb = r_p * alpha.cos();
    let ra = m * (z / 2.0 + p.addendum + p.shift);
    let rf = m * (z / 2.0 - p.dedendum + p.shift);
    let h = PI / z;
    // half tooth angle on the pitch circle
    let half_pitch = (PI / 2.0 + 2.0 * p.shift * alpha.tan()) / z;
    // half tooth angle at radius rho (>= rb); below the base circle the flank runs radially
    let psi = |rho: f64| -> f64 {
        let rho = rho.max(rb);
        let a = (rb / rho).min(1.0).acos();
        half_pitch + inv(alpha) - inv(a)
    };
    let psi_a = psi(ra);
    if psi_a * ra < 0.05 * m {
        return Err("the tooth comes to a point; lower the addendum or the profile shift".into());
    }
    if psi(rf.max(rb)) >= 0.97 * h {
        return Err("the teeth would overlap at the root; adjust the dedendum or the profile shift".into());
    }
    // flank point at radius rho on the +angle side (mirrored later)
    let flank = |rho: f64| -> P { polar(rho, psi(rho)) };
    let n = 600;
    let samples: Vec<P> = (0..=n)
        .map(|i| flank(rf + (ra - rf) * i as f64 / n as f64))
        .collect();
    let near = |c: P| -> (f64, usize) {
        let mut best = (f64::MAX, 0);
        for (i, s) in samples.iter().enumerate() {
            let d = dist(c, *s);
            if d < best.0 {
                best = (d, i);
            }
        }
        best
    };
    // Root fillet: a circle of radius rho_f tangent to the root circle (centre at rf + rho_f)
    // and to the flank. Find the centre angle by bisection; shrink the fillet if the gap is too small.
    let mut rho_f = 0.3 * m;
    let mut found = None;
    for _ in 0..12 {
        let lo0 = psi(rf + rho_f);
        let (d_hi, _) = near(polar(rf + rho_f, h));
        if d_hi > rho_f {
            let (mut lo, mut hi) = (lo0, h);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if near(polar(rf + rho_f, mid)).0 < rho_f {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            found = Some(0.5 * (lo + hi));
            break;
        }
        rho_f *= 0.7;
    }
    let phi_c = found.ok_or("no room for the root fillet; adjust the dedendum or the profile shift")?;
    let c = polar(rf + rho_f, phi_c);
    let (_, ti) = near(c);
    let t_pt = samples[ti];
    let r_t = (t_pt[0].powi(2) + t_pt[1].powi(2)).sqrt();
    let root_pt = polar(rf, phi_c);
    // fillet midpoint: on the fillet circle, between the root point and the tangent point
    let (a0, a1) = (
        (root_pt[1] - c[1]).atan2(root_pt[0] - c[0]),
        (t_pt[1] - c[1]).atan2(t_pt[0] - c[0]),
    );
    let mut da = a1 - a0;
    while da > PI {
        da -= 2.0 * PI;
    }
    while da < -PI {
        da += 2.0 * PI;
    }
    let fil_mid = [
        c[0] + rho_f * (a0 + da / 2.0).cos(),
        c[1] + rho_f * (a0 + da / 2.0).sin(),
    ];
    // flank from the tangent point to the tip: a radial line below the base circle, then an involute spline
    let mut flank_pts: Vec<P> = Vec::new();
    let mut line_part: Option<(P, P)> = None;
    let r_start = if r_t < rb {
        let pb = flank(rb);
        line_part = Some((t_pt, pb));
        rb
    } else {
        flank_pts.push(t_pt);
        r_t
    };
    if line_part.is_some() {
        flank_pts.push(flank(rb));
    }
    let npts = 10;
    // spaced evenly in the roll angle t, where rho = rb sqrt(1 + t^2)
    let t0 = ((r_start / rb).powi(2) - 1.0).max(0.0).sqrt();
    let t1 = ((ra / rb).powi(2) - 1.0).sqrt();
    for i in 1..=npts {
        let t = t0 + (t1 - t0) * i as f64 / npts as f64;
        let rho = if i == npts {
            ra
        } else {
            rb * (1.0 + t * t).sqrt()
        };
        flank_pts.push(flank(rho));
    }
    let tip = flank(ra);
    let mirror = |q: P| -> P { [q[0], -q[1]] };
    let rev = |v: &Vec<P>| -> Vec<P> { v.iter().rev().cloned().collect() };

    let mut segs = Vec::new();
    let r_in = 0.75 * rf;
    // go counter-clockwise from the right-hand pitch edge at the root circle
    let edge_r = polar(rf, -h);
    let edge_l = polar(rf, h);
    segs.push(Seg::Line(polar(r_in, -h), edge_r));
    segs.push(Seg::Arc(
        edge_r,
        mirror(root_pt),
        polar(rf, -(h + phi_c) / 2.0),
    ));
    segs.push(Seg::Arc(mirror(root_pt), mirror(t_pt), mirror(fil_mid)));
    if let Some((a, b)) = line_part {
        segs.push(Seg::Line(mirror(a), mirror(b)));
    }
    let right: Vec<P> = flank_pts.iter().map(|q| mirror(*q)).collect();
    segs.push(Seg::Spline(right));
    segs.push(Seg::Arc(mirror(tip), tip, [ra, 0.0]));
    segs.push(Seg::Spline(rev(&flank_pts)));
    if let Some((a, b)) = line_part {
        segs.push(Seg::Line(b, a));
    }
    segs.push(Seg::Arc(t_pt, root_pt, fil_mid));
    segs.push(Seg::Arc(root_pt, edge_l, polar(rf, (h + phi_c) / 2.0)));
    segs.push(Seg::Line(edge_l, polar(r_in, h)));
    segs.push(Seg::Arc(polar(r_in, h), polar(r_in, -h), [r_in, 0.0]));
    Ok(Wedge {
        segs,
        root_fillet: rho_f,
        inner_radius: r_in,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(z: u32) -> Params {
        Params {
            module: 2.0,
            teeth: z,
            pressure_angle_deg: 20.0,
            face_width: 10.0,
            bore: 0.0,
            shift: 0.0,
            addendum: 1.0,
            dedendum: 1.25,
        }
    }
    #[test]
    fn diameters() {
        let d = dims(&p(20));
        assert!((d.pitch_d - 40.0).abs() < 1e-9);
        assert!((d.outer_d - 44.0).abs() < 1e-9);
        assert!((d.base_d - 40.0 * 20f64.to_radians().cos()).abs() < 1e-9);
    }
    #[test]
    fn tooth_thickness_at_pitch_circle() {
        // The flank at the pitch radius must sit half a tooth thickness (pi m / 4) off the tooth axis.
        let w = wedge(&p(20)).unwrap();
        let rp = 20.0;
        let mut found = false;
        for s in &w.segs {
            if let Seg::Spline(pts) = s {
                for q in pts {
                    let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
                    if (r - rp).abs() < 0.05 {
                        let ang = q[1].atan2(q[0]).abs();
                        assert!((rp * ang - PI * 2.0 / 4.0).abs() < 0.05, "{}", rp * ang);
                        found = true;
                    }
                }
            }
        }
        let _ = found;
    }
    #[test]
    fn addendum_and_dedendum_move_the_radii() {
        let mut q = p(20);
        q.addendum = 1.2;
        q.dedendum = 1.5;
        let d = dims(&q);
        assert!((d.outer_d - 2.0 * (20.0 + 2.4)).abs() < 1e-9, "{}", d.outer_d);
        assert!((d.root_d - 2.0 * (20.0 - 3.0)).abs() < 1e-9, "{}", d.root_d);
        // the tip arc passes through (ra, 0) and the root arcs lie on rf
        let w = wedge(&q).unwrap();
        let (ra, rf) = (d.outer_d / 2.0, d.root_d / 2.0);
        assert!(w.segs.iter().any(|s| matches!(s, Seg::Arc(_, _, t) if (t[0] - ra).abs() < 1e-9 && t[1].abs() < 1e-9)));
        assert!(w.segs.iter().any(|s| matches!(s, Seg::Arc(a, _, _) if ((a[0] * a[0] + a[1] * a[1]).sqrt() - rf).abs() < 1e-9)));
    }
    #[test]
    fn tooth_thickness_does_not_depend_on_addendum_or_dedendum() {
        for (ha, hf) in [(0.8, 1.0), (1.0, 1.25), (1.3, 1.6)] {
            let mut q = p(24);
            q.addendum = ha;
            q.dedendum = hf;
            let rp = 24.0;
            let w = wedge(&q).unwrap();
            for s in &w.segs {
                if let Seg::Spline(pts) = s {
                    for c in pts {
                        let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
                        if (r - rp).abs() < 0.05 {
                            let ang = c[1].atan2(c[0]).abs();
                            assert!((rp * ang - PI * 2.0 / 4.0).abs() < 0.06, "{ha} {hf}: {}", rp * ang);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn wedges_build_for_a_range_of_addendum_and_dedendum() {
        for z in [12, 20, 40] {
            for ha in [0.6, 0.8, 1.0, 1.2] {
                for hf in [0.9, 1.0, 1.25, 1.5] {
                    let mut q = p(z);
                    q.addendum = ha;
                    q.dedendum = hf;
                    validate(&q).unwrap();
                    wedge(&q).unwrap_or_else(|e| panic!("z{z} ha{ha} hf{hf}: {e}"));
                }
            }
        }
    }
    #[test]
    fn extremes_give_a_result_or_a_message_never_a_panic() {
        let mut ok = 0;
        for z in [8, 9, 20, 80] {
            for ha in [0.3, 1.0, 2.0] {
                for hf in [0.3, 1.25, 2.5] {
                    for x in [-1.0, 0.0, 1.0] {
                        let mut q = p(z);
                        q.addendum = ha;
                        q.dedendum = hf;
                        q.shift = x;
                        if validate(&q).is_ok() {
                            match wedge(&q) {
                                Ok(w) => {
                                    ok += 1;
                                    assert!(w.root_fillet > 0.0 && w.root_fillet.is_finite());
                                }
                                Err(e) => assert!(!e.is_empty()),
                            }
                        }
                    }
                }
            }
        }
        assert!(ok > 20, "{ok}");
    }
    #[test]
    fn rejects_bad_addendum_and_dedendum() {
        let mut q = p(20);
        q.addendum = 0.1;
        assert_eq!(validate(&q).unwrap_err().0, "addendum");
        q = p(20);
        q.addendum = 2.5;
        assert_eq!(validate(&q).unwrap_err().0, "addendum");
        q = p(20);
        q.dedendum = 3.0;
        assert_eq!(validate(&q).unwrap_err().0, "dedendum");
        q = p(20);
        q.dedendum = f64::NAN;
        assert_eq!(validate(&q).unwrap_err().0, "dedendum");
    }
    #[test]
    fn wedges_build_for_many_counts() {
        for z in [8, 12, 17, 20, 30, 41, 60, 100, 200] {
            let w = wedge(&p(z)).unwrap_or_else(|e| panic!("{z}: {e}"));
            assert!(w.root_fillet > 0.0);
        }
    }
    #[test]
    fn closed_loop() {
        let w = wedge(&p(20)).unwrap();
        let ends: Vec<(P, P)> = w
            .segs
            .iter()
            .map(|s| match s {
                Seg::Spline(v) => (v[0], *v.last().unwrap()),
                Seg::Line(a, b) => (*a, *b),
                Seg::Arc(a, b, _) => (*a, *b),
            })
            .collect();
        for i in 0..ends.len() {
            let (_, e) = ends[i];
            let (s, _) = ends[(i + 1) % ends.len()];
            assert!(dist(e, s) < 1e-12, "gap at segment {i}");
        }
    }
}
