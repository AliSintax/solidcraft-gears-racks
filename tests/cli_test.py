#!/usr/bin/env python3
"""End to end test: runs the built plugin through solidcraft-cli and measures the gear.

    SOLIDCRAFT_CLI=/path/to/solidcraft-cli tests/cli_test.py [package-dir]

The package dir holds plugin.json and plugin.wasm (default: dist/solidcraft-gears).
Needs a solidcraft-cli with the plugin host (SolidCraft 0.41 or newer).
"""
import json, math, os, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLI = os.environ.get("SOLIDCRAFT_CLI", "solidcraft-cli")
PKG = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "dist", "solidcraft-gears"))
TMP = tempfile.mkdtemp(prefix="gears-test-")


def run(steps, save=None, inp=None):
    script = os.path.join(TMP, "s.json")
    out = os.path.join(TMP, "o.json")
    json.dump(steps, open(script, "w"))
    cmd = [CLI, "run", script, "--results", "--quiet", "--json-out", out]
    if inp:
        cmd += ["--in", inp]
    if save:
        cmd += ["--save", save]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit("cli failed: " + p.stderr[-2000:] + p.stdout[-500:])
    return [r.get("result", r) if isinstance(r, dict) else r for r in json.load(open(out))]


def approval():
    r = run([{"command": "plugin.inspect", "params": {"path": PKG}}])[0]
    return r["approval"]


APPROVAL = approval()


def gear(params):
    return {"command": "plugin.run", "params": {"path": PKG, "approval": APPROVAL, "entry": "make-gear", "params": params}}


def check(name, ok, detail=""):
    print(("PASS " if ok else "FAIL ") + name + (" " + detail if detail else ""))
    if not ok:
        check.failed = True


check.failed = False


def cylinders(faces):
    return [f["cylinder"]["radius"] for f in faces if f.get("cylinder")]


def case(label, p, save=None):
    m, z = p.get("module", 2), p["teeth"]
    x = p.get("profile_shift", 0)
    bore = p.get("bore_diameter", 0)
    steps = [gear(p), {"command": "inspect.measure", "params": {}}, {"command": "model.faces", "params": {"body": "Gear"}}]
    res = run(steps, save=save)
    body = res[1]["bodies"][0]
    faces = res[2]["faces"] if isinstance(res[2], dict) else res[2]
    radii = cylinders(faces)
    ra, rf = m * (z + 2 + 2 * x) / 2, m * (z - 2.5 + 2 * x) / 2
    tips = [r for r in radii if abs(r - ra) < 1e-6]
    roots = [r for r in radii if abs(r - rf) < 1e-6]
    check(f"{label}: one closed solid", len(res[1]["bodies"]) == 1 and body["closed"])
    check(f"{label}: tooth count (tip faces) = {z}", len(tips) == z, f"got {len(tips)}")
    check(f"{label}: root faces (one or two per gap, cut by the pattern seam) for {z} gaps", len(roots) in (z, 2 * z), f"got {len(roots)}")
    check(f"{label}: outer diameter = m(z+2+2x) = {2*ra:g}", abs(2 * max(radii) - 2 * ra) < 1e-6, f"got {2*max(radii):.6f}")
    bb = body["bbox"]
    span = max(bb["max"][0] - bb["min"][0], bb["max"][1] - bb["min"][1])
    check(f"{label}: bounding box within outer diameter", span <= 2 * ra + 1e-6 and span > 2 * ra * 0.97, f"{span:.4f}")
    if bore:
        check(f"{label}: bore diameter = {bore:g}", any(abs(2 * r - bore) < 1e-6 for r in radii))
    check(f"{label}: face width", abs(bb["max"][2] - bb["min"][2] - p.get("face_width", 10)) < 1e-6)
    return res


def pitch_check(label, p, save):
    """At the pitch circle (diameter m z) the tooth and the space are each half the circumference
    when there is no profile shift. Cut a thin ring around d = m z out of the gear and measure
    how much of it is material."""
    m, z = p.get("module", 2), p["teeth"]
    x = p.get("profile_shift", 0)
    a = math.radians(p.get("pressure_angle", 20))
    rp, w, h = m * z / 2, 0.02, p.get("face_width", 10)
    steps = [
        {"command": "solid.cylinder", "params": {"radius": rp + w, "height": h, "operation": "new", "body_name": "Ring"}},
        {"command": "solid.cylinder", "params": {"radius": rp - w, "height": h, "operation": "cut", "targets": ["Ring"]}},
        {"command": "solid.combine", "params": {"target": "Ring", "tools": ["Gear"], "operation": "intersect", "keep_tools": True}},
        {"command": "inspect.measure", "params": {}},
    ]
    res = run(steps, inp=save)
    # the intersection leaves one piece of ring per tooth: add them up
    ring = sum(b["volume_mm3"] for b in res[-1]["bodies"] if b["name"].startswith("Ring"))
    full = math.pi * ((rp + w) ** 2 - (rp - w) ** 2) * h
    frac = ring / full
    expect = (math.pi / 2 + 2 * x * math.tan(a)) / math.pi  # tooth thickness s over circular pitch
    check(f"{label}: tooth fills {expect:.3f} of the ring at d = m z = {2*rp:g}", abs(frac - expect) < 0.005, f"got {frac:.4f}")


os.makedirs(os.path.join(TMP, "out"), exist_ok=True)
for label, p in {
    "z20 m2 bore6": dict(module=2, teeth=20, face_width=10, bore_diameter=6),
    "z24 m1.5": dict(module=1.5, teeth=24, face_width=8),
    "z17 m3 shift0.3": dict(module=3, teeth=17, face_width=12, bore_diameter=8, profile_shift=0.3),
    "z40 m1 a25": dict(module=1, teeth=40, pressure_angle=25, face_width=5, bore_diameter=5),
    "z9 m5": dict(module=5, teeth=9, face_width=15, bore_diameter=10),
    "z72 m1": dict(module=1, teeth=72, face_width=4, bore_diameter=3),
}.items():
    sc = os.path.join(TMP, "out", label.split()[0] + ".sc")
    case(label, p, save=sc)
    pitch_check(label, p, sc)
    reopened = run([{"command": "inspect.measure", "params": {}}], inp=sc)[0]
    check(f"{label}: saved .sc reopens with one closed body", len(reopened["bodies"]) == 1 and reopened["bodies"][0]["closed"])

import shutil
shutil.rmtree(TMP)
sys.exit(1 if check.failed else 0)
