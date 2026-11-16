//! Involute spur gear generator for SolidCraft.
//!
//! Two entries, `gears` and `racks`, each building its part in its own component. `gears` draws the outline of a single tooth wedge (involute flanks as
//! splines, a root fillet, tip and root arcs), extrudes it onto a hub cylinder, then repeats the
//! tooth with a circular pattern and cuts the bore. Every step is an ordinary SolidCraft command,
//! so the gear lands on the timeline as editable features.
//! `racks` draws the whole rack outline as one polyline sketch and extrudes it once.

pub mod gear;
pub mod rack;

use gear::{Params, Seg};
use rack::RackParams;
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
pub fn make_gear<R: Runner>(r: &mut R, p: &Params, name: &str) -> Result<Value, String> {
    gear::validate(p).map_err(|(f, why)| format!("{f}: {why}"))?;
    let w = gear::wedge(p)?;
    let d = gear::dims(p);
    // Its own component, active while the gear is built, so everything lands inside it.
    r.run("component.create", json!({"name": name, "parent": "root", "activate": true}))?;
    // The hub is a plain cylinder; the tooth wedge overlaps it and joins into one body.
    let tooth = format!("{name} tooth");
    let hub_r = 0.85 * (d.root_d / 2.0);
    r.run("solid.cylinder", json!({"radius": hub_r, "height": p.face_width, "operation": "new", "name": name, "body_name": name}))?;
    r.run(
        "sketch.create",
        json!({"plane": "XY", "name": format!("{name} tooth profile")}),
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
        json!({"distance": p.face_width, "profiles": "all", "operation": "join", "name": &tooth}),
    )?;
    r.run(
        "solid.pattern.circular",
        json!({"features": [&tooth], "axis": "Z", "count": p.teeth}),
    )?;
    if p.bore > 0.0 {
        r.run(
            "solid.cylinder",
            json!({"diameter": p.bore, "height": p.face_width, "operation": "cut", "name": format!("{name} bore")}),
        )?;
    }
    r.run("component.activate", json!({"component": "root"}))?;
    Ok(json!({
        "component": name,
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

pub fn parse_rack_params(v: &Value) -> RackParams {
    let num = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d);
    RackParams {
        module: num("module", 2.0),
        teeth: num("teeth", 10.0).round().max(0.0) as u32,
        pressure_angle_deg: num("pressure_angle", 20.0),
        face_width: num("face_width", 10.0),
        body_height: num("body_height", 6.0),
    }
}

/// Build the rack: one closed polyline sketch on XY, extruded up along +Z.
pub fn make_rack<R: Runner>(r: &mut R, p: &RackParams, name: &str) -> Result<Value, String> {
    rack::validate(p).map_err(|(f, why)| format!("{f}: {why}"))?;
    let d = rack::dims(p);
    let pts = rack::outline(p);
    r.run("component.create", json!({"name": name, "parent": "root", "activate": true}))?;
    r.run("sketch.create", json!({"plane": "XY", "name": format!("{name} profile")}))?;
    for i in 0..pts.len() {
        r.run("sketch.line", json!({"points": [pts[i], pts[(i + 1) % pts.len()]]}))?;
    }
    r.run("sketch.finish", json!({}))?;
    r.run(
        "solid.extrude",
        json!({"distance": p.face_width, "profiles": "all", "operation": "new", "name": name, "body_name": name}),
    )?;
    r.run("component.activate", json!({"component": "root"}))?;
    Ok(json!({
        "component": name,
        "teeth": p.teeth,
        "module": p.module,
        "pitch": d.pitch,
        "length": d.length,
        "tip_y": d.tip_y,
        "root_y": d.root_y,
        "total_height": d.tip_y - d.bottom_y,
        "tip_width": d.tip_width,
        "face_width": p.face_width,
    }))
}

/// What one run of an entry asks for.
#[derive(Clone, Debug)]
pub enum Request {
    Gear(Params, String),
    Rack(RackParams, String),
}

/// The component (and body) name: the `name` parameter, or `default` when it is left out.
fn part_name(v: &Value, default: &str) -> Result<String, (String, String)> {
    match v.get("name").and_then(Value::as_str).map(str::trim) {
        None | Some("") => Ok(default.to_string()),
        Some(n) if n.chars().count() <= 64 => Ok(n.to_string()),
        Some(_) => Err(("name".into(), "must be at most 64 characters".into())),
    }
}

/// The entry ids, as in `plugin.json`. The host wants lowercase ids (letters, digits, hyphens).
pub const GEARS: &str = "gears";
pub const RACKS: &str = "racks";

/// Read and validate the parameters of one entry (`gears` or `racks`).
pub fn parse_request(entry: &str, v: &Value) -> Result<Request, (String, String)> {
    match entry {
        GEARS => {
            let p = parse_params(v);
            gear::validate(&p)?;
            Ok(Request::Gear(p, part_name(v, "Gear")?))
        }
        RACKS => {
            let p = parse_rack_params(v);
            rack::validate(&p)?;
            Ok(Request::Rack(p, part_name(v, "Rack")?))
        }
        other => Err(("entry".into(), format!("unknown entry {other:?}"))),
    }
}

pub fn make<R: Runner>(r: &mut R, req: &Request) -> Result<Value, String> {
    match req {
        Request::Gear(p, name) => make_gear(r, p, name),
        Request::Rack(p, name) => make_rack(r, p, name),
    }
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
                "gears" | "racks" => {
                    let v: Value = serde_json::from_str(&params).unwrap_or(json!({}));
                    let req = match parse_request(entry.as_str(), &v) {
                        Ok(r) => r,
                        Err((field, reason)) => {
                            return Err(PluginError::InvalidInput(FieldFailure { field, reason }))
                        }
                    };
                    match &req {
                        Request::Gear(p, _) => log::log(
                            log::Level::Info,
                            &format!("making a {}-tooth gear, module {}", p.teeth, p.module),
                        ),
                        Request::Rack(p, _) => log::log(
                            log::Level::Info,
                            &format!("making a {}-tooth rack, module {}", p.teeth, p.module),
                        ),
                    }
                    match make(&mut Host, &req) {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Rec(Vec<(String, Value)>);
    impl Runner for Rec {
        fn run(&mut self, command: &str, args: Value) -> Result<Value, String> {
            self.0.push((command.to_string(), args));
            Ok(Value::Null)
        }
    }

    #[test]
    fn rack_command_sequence() {
        let p = parse_rack_params(&json!({"module": 2, "teeth": 5}));
        let mut r = Rec(vec![]);
        let out = make_rack(&mut r, &p, "Rack").unwrap();
        let names: Vec<&str> = r.0.iter().map(|(c, _)| c.as_str()).collect();
        // component, sketch, 4 lines per tooth + 4 for the ends and body, finish, extrude, back to root
        assert_eq!(names.first(), Some(&"component.create"));
        assert_eq!(names[1], "sketch.create");
        assert_eq!(names.iter().filter(|c| **c == "sketch.line").count(), 4 * 5 + 4);
        assert_eq!(names[names.len() - 3], "sketch.finish");
        assert_eq!(names[names.len() - 2], "solid.extrude");
        assert_eq!(names.last(), Some(&"component.activate"));
        assert!((out["length"].as_f64().unwrap() - 5.0 * PI_2()).abs() < 1e-9);
        // every line joins the previous one end to start (closed loop)
        let lines: Vec<&Value> = r.0.iter().filter(|(c, _)| c == "sketch.line").map(|(_, a)| a).collect();
        for i in 0..lines.len() {
            assert_eq!(lines[i]["points"][1], lines[(i + 1) % lines.len()]["points"][0]);
        }
    }

    #[test]
    fn entry_selects_builder() {
        assert!(matches!(parse_request(GEARS, &json!({})), Ok(Request::Gear(..))));
        assert!(matches!(parse_request(GEARS, &json!({"teeth": 20})), Ok(Request::Gear(..))));
        assert!(matches!(parse_request(RACKS, &json!({"teeth": 3})), Ok(Request::Rack(..))));
        assert_eq!(parse_request("make-gear", &json!({})).unwrap_err().0, "entry");
        // each entry applies its own limits: 3 teeth is fine for a rack, too few for a gear
        assert_eq!(parse_request(GEARS, &json!({"teeth": 3})).unwrap_err().0, "teeth");
        assert_eq!(parse_request(RACKS, &json!({"teeth": 101})).unwrap_err().0, "teeth");
    }

    #[test]
    fn make_dispatches() {
        let mut r = Rec(vec![]);
        make(&mut r, &parse_request(RACKS, &json!({"teeth": 2})).unwrap()).unwrap();
        assert!(r.0.iter().all(|(c, _)| c.starts_with("sketch.") || c.starts_with("component.") || c == "solid.extrude"));
        let mut g = Rec(vec![]);
        make(&mut g, &parse_request(GEARS, &json!({})).unwrap()).unwrap();
        assert!(g.0.iter().any(|(c, _)| c == "solid.pattern.circular"));
    }

    #[test]
    fn each_part_gets_its_own_component() {
        for (entry, v, default) in [(GEARS, json!({}), "Gear"), (RACKS, json!({"teeth": 2}), "Rack")] {
            for given in [None, Some("Pinion 1")] {
                let mut v = v.clone();
                let want = given.unwrap_or(default);
                if let Some(n) = given {
                    v["name"] = json!(n);
                }
                let mut r = Rec(vec![]);
                make(&mut r, &parse_request(entry, &v).unwrap()).unwrap();
                let (c0, a0) = &r.0[0];
                assert_eq!(c0, "component.create");
                assert_eq!(a0, &json!({"name": want, "parent": "root", "activate": true}));
                // only one component is made, and the run ends back in the root
                assert_eq!(r.0.iter().filter(|(c, _)| c == "component.create").count(), 1);
                let (cl, al) = r.0.last().unwrap();
                assert_eq!(cl, "component.activate");
                assert_eq!(al, &json!({"component": "root"}));
                // nothing is built before the component exists
                assert!(r.0[1..].iter().all(|(c, _)| c != "component.create"));
            }
        }
    }

    #[test]
    fn gear_features_are_named_after_the_part() {
        let mut r = Rec(vec![]);
        let mut v = json!({"bore_diameter": 6});
        v["name"] = json!("Pinion");
        make(&mut r, &parse_request(GEARS, &v).unwrap()).unwrap();
        let args = |cmd: &str| -> Vec<&Value> { r.0.iter().filter(|(c, _)| c == cmd).map(|(_, a)| a).collect() };
        let pat = args("solid.pattern.circular");
        assert_eq!(pat[0]["features"], json!(["Pinion tooth"]));
        let ext = args("solid.extrude");
        assert_eq!(ext[0]["name"], "Pinion tooth");
        let cyl = args("solid.cylinder");
        assert_eq!(cyl[0]["body_name"], "Pinion");
        assert_eq!(cyl[1]["name"], "Pinion bore");
    }

    #[test]
    fn name_is_checked() {
        assert_eq!(parse_request(GEARS, &json!({"name": "x".repeat(65)})).unwrap_err().0, "name");
        assert_eq!(parse_request(RACKS, &json!({"name": "x".repeat(65)})).unwrap_err().0, "name");
        // blank falls back to the default
        match parse_request(GEARS, &json!({"name": "  "})).unwrap() {
            Request::Gear(_, n) => assert_eq!(n, "Gear"),
            _ => panic!(),
        }
        match parse_request(RACKS, &json!({})).unwrap() {
            Request::Rack(_, n) => assert_eq!(n, "Rack"),
            _ => panic!(),
        }
    }

    #[test]
    fn manifest_description_fits_the_host_limit() {
        // the host refuses a manifest description over 240 printable characters
        let m: Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
        let d = m["description"].as_str().unwrap();
        assert!(d.chars().count() <= 240, "{}", d.chars().count());
        assert!(d.chars().all(|c| !c.is_control()));
    }

    #[test]
    fn manifest_entries_and_field_order() {
        let text = include_str!("../plugin.json");
        let m: Value = serde_json::from_str(text).unwrap();
        // the plugin is called Gears; its two entries have lowercase ids the host accepts
        assert_eq!(m["name"], "Gears");
        let entries = m["runtime"]["entries"].as_array().unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, [GEARS, RACKS]);
        for n in &names {
            let mut c = n.chars();
            assert!(c.next().is_some_and(|f| f.is_ascii_lowercase()), "{n}");
            assert!(c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '-'), "{n}");
        }
        // the menu item of each entry is labelled by its slot name, not by its id
        let labels: Vec<&str> = entries.iter().map(|e| e["slots"][0]["name"].as_str().unwrap()).collect();
        assert_eq!(labels, ["Gears", "Racks"]);
        // The host draws the required fields first, in the order of `required`, and the optional
        // ones after them in alphabetical order. So the name has to be a required field, first.
        let required = |e: &Value| -> Vec<String> {
            e["input_schema"]["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
        };
        assert_eq!(required(&entries[0]), ["name", "module", "teeth", "face_width", "pressure_angle"]);
        assert_eq!(required(&entries[1]), ["name", "module", "teeth", "pressure_angle", "face_width", "body_height"]);
        for (e, default) in entries.iter().zip(["Gear", "Rack"]) {
            let props = &e["input_schema"]["properties"];
            // a required name needs a default so the form is valid as it opens
            assert_eq!(props["name"]["default"], default);
            for k in required(e) {
                assert!(props.get(&k).is_some(), "required {k} has no property");
            }
        }
        // fields that do not apply to a part are not offered for it
        let props = |i: usize| entries[i]["input_schema"]["properties"].as_object().unwrap().clone();
        assert!(props(0).get("body_height").is_none());
        assert!(props(1).get("bore_diameter").is_none() && props(1).get("profile_shift").is_none());
        assert!(!text.contains("\"enum\""));
    }

    #[allow(non_snake_case)]
    fn PI_2() -> f64 {
        2.0 * std::f64::consts::PI
    }

    #[test]
    fn rack_rejects_invalid() {
        let p = parse_rack_params(&json!({"teeth": 0}));
        assert!(make_rack(&mut Rec(vec![]), &p, "Rack").is_err());
    }
}
