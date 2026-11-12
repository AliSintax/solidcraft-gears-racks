//! Involute spur gear generator for SolidCraft.
//!
//! One entry, `make-gear`. It draws the outline of a single tooth wedge (involute flanks as
//! splines, a root fillet, tip and root arcs), extrudes it onto a hub cylinder, then repeats the
//! tooth with a circular pattern and cuts the bore. Every step is an ordinary SolidCraft command,
//! so the gear lands on the timeline as editable features.

pub mod gear;

use gear::{Params, Seg};
use serde_json::{json, Value};

/// What a step needs from the host: run one command with JSON args, get the JSON reply.
pub trait Runner {
    fn run(&mut self, command: &str, args: Value) -> Result<Value, String>;
}

pub fn parse_params(v: &Value) -> Params {
    let num = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d);
    Params {
        module: num("module", 2.0),
        teeth: num("teeth", 20.0).round().max(0.0) as u32,
        pressure_angle_deg: num("pressure_angle", 20.0),
        face_width: num("face_width", 10.0),
        bore: num("bore_diameter", 0.0),
        shift: num("profile_shift", 0.0),
    }
}

/// Build the gear; returns a summary of what was made.
pub fn make_gear<R: Runner>(r: &mut R, p: &Params) -> Result<Value, String> {
    gear::validate(p).map_err(|(f, why)| format!("{f}: {why}"))?;
    let w = gear::wedge(p)?;
    let d = gear::dims(p);
    // The hub is a plain cylinder; the tooth wedge overlaps it and joins into one body.
    let hub_r = 0.85 * (d.root_d / 2.0);
    r.run("solid.cylinder", json!({"radius": hub_r, "height": p.face_width, "operation": "new", "name": "Gear", "body_name": "Gear"}))?;
    r.run(
        "sketch.create",
        json!({"plane": "XY", "name": "Tooth profile"}),
    )?;
    for s in &w.segs {
        match s {
            Seg::Spline(pts) => r.run("sketch.spline.fit_point", json!({"points": pts}))?,
            Seg::Line(a, b) => r.run("sketch.line", json!({"points": [a, b]}))?,
            Seg::Arc(a, b, t) => r.run(
                "sketch.arc.three_point",
                json!({"start": a, "end": b, "through": t}),
            )?,
        };
    }
    r.run("sketch.finish", json!({}))?;
    r.run(
        "solid.extrude",
        json!({"distance": p.face_width, "profiles": "all", "operation": "join", "name": "Tooth"}),
    )?;
    r.run(
        "solid.pattern.circular",
        json!({"features": ["Tooth"], "axis": "Z", "count": p.teeth}),
    )?;
    if p.bore > 0.0 {
        r.run(
            "solid.cylinder",
            json!({"diameter": p.bore, "height": p.face_width, "operation": "cut", "name": "Bore"}),
        )?;
    }
    Ok(json!({
        "teeth": p.teeth,
        "module": p.module,
        "pitch_diameter": d.pitch_d,
        "base_diameter": d.base_d,
        "outer_diameter": d.outer_d,
        "root_diameter": d.root_d,
        "face_width": p.face_width,
        "bore_diameter": p.bore,
        "root_fillet_radius": w.root_fillet,
    }))
}

#[cfg(target_arch = "wasm32")]
mod guest {
    wit_bindgen::generate!({ path: "wit", world: "command-plugin" });

    use super::*;
    use craft::plugin::{
        commands, log,
        types::{FieldFailure, PluginError},
    };

    struct Host;
    impl Runner for Host {
        fn run(&mut self, command: &str, args: Value) -> Result<Value, String> {
            match commands::run(command, &args.to_string()) {
                Ok(s) => Ok(serde_json::from_str(&s).unwrap_or(Value::Null)),
                Err(e) => Err(format!("{command}: {e:?}")),
            }
        }
    }

    struct Gears;
    impl exports::craft::plugin::entry::Guest for Gears {
        fn execute(entry: String, params: String) -> Result<String, PluginError> {
            match entry.as_str() {
                "make-gear" => {
                    let v: Value = serde_json::from_str(&params).unwrap_or(json!({}));
                    let p = parse_params(&v);
                    if let Err((field, reason)) = gear::validate(&p) {
                        return Err(PluginError::InvalidInput(FieldFailure { field, reason }));
                    }
                    log::log(
                        log::Level::Info,
                        &format!("making a {}-tooth gear, module {}", p.teeth, p.module),
                    );
                    match make_gear(&mut Host, &p) {
                        Ok(v) => Ok(v.to_string()),
                        Err(e) => Err(PluginError::Trap(e)),
                    }
                }
                other => Err(PluginError::InvalidInput(FieldFailure {
                    field: "entry".into(),
                    reason: format!("unknown entry {other}"),
                })),
            }
        }
    }
    export!(Gears);
}
