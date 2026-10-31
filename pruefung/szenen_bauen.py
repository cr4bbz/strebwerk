"""Baut die Prüfszenen über den MCP-Server (wie ein Sprachmodell es täte) und sammelt
alle Textausgaben (analyze, sections, interactions) als JSON."""
import json, os, shutil, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BIN = os.environ.get("STREBWERK_BIN", str(REPO / "target" / "release" / ("strebwerk.exe" if os.name == "nt" else "strebwerk")))
OUT = Path(sys.argv[1])
ROOT = OUT / "szenen"
ROOT.mkdir(parents=True, exist_ok=True)
for ex in ["anti-rho3-n4", "ikosaeder-dodekaeder", "interaktion"]:
    shutil.copy(REPO / "examples" / f"{ex}.strebwerk", ROOT / f"{ex}.strebwerk")


class Mcp:
    def __init__(self):
        self.p = subprocess.Popen([BIN, "--mcp", "--root", str(ROOT)], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.i = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "pruef", "version": "0"}})
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")

    def rpc(self, method, params):
        self.i += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.i, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())

    def tool(self, _tool, **args):
        r = self.rpc("tools/call", {"name": _tool, "arguments": args})["result"]
        txt = r["content"][0]["text"]
        if r["isError"]:
            raise RuntimeError(f"{_tool}: {txt}")
        return txt


m = Mcp()
cube = {"Polyhedron": {"base": {"Platonic": {"kind": "Cube", "a": "2+2*rho"}}, "ops": []}}
P = {"O": [0, 0, 0], "a": ["1", "rho", "rho^3"], "b": ["1", "rho^3", "rho"], "c": ["rho", "1", "rho^3"],
     "d": ["rho", "rho^3", "1"], "e": ["rho^3", "1", "rho"]}

scenes = {}

# S1: Würfel 2ρ³ und Kegel mit Spitze im Mittelpunkt, Schnitt x = s
m.tool("new_scene")
m.tool("add_object", name="Würfel", kind=cube, layers={"space_diag": True})
m.tool("add_points", name="Kegel", points=[P["O"], P["a"], P["d"], P["b"]], layers={"spokes": True})
m.tool("set_param", name="s", value=0.5, min=0, max=2.4)
m.tool("add_section", axis="x", offset="s")
m.tool("save_scene", path="pruef-1-wuerfel-kegel.strebwerk")
scenes["pruef-1-wuerfel-kegel"] = None

# S4: die vier Volumenklassen der Kegel über dem Permutationssechseck
m.tool("new_scene")
for name, pts in [("Kegel ρ⁻¹", "abc"), ("Kegel 1", "acd"), ("Kegel ρ⁴", "abe"), ("Kegel 2+ρ (gerade)", "ade")]:
    m.tool("add_points", name=name, points=[P["O"]] + [P[k] for k in pts])
P["f"] = ["rho^3", "rho", "1"]
# Sechseck in Umlaufreihenfolge: abwechselnd Tausch ρ↔ρ³ (Kante √2) und 1↔ρ (Kante √2·ρ⁻⁴)
m.tool("add_points", name="Sechseck", hull=False, points=[P[k] for k in "abdfec"],
       struts=[[i, (i + 1) % 6] for i in range(6)])
m.tool("save_scene", path="pruef-4-sechseck.strebwerk")
scenes["pruef-4-sechseck"] = None

for ex in ["anti-rho3-n4", "ikosaeder-dodekaeder", "interaktion"]:
    scenes[ex] = None

res = {}
for name in scenes:
    m.tool("load_scene", path=f"{name}.strebwerk")
    res[name] = {
        "info": m.tool("scene_info"),
        "analyze": m.tool("analyze"),
        "sections": m.tool("sections"),
        "interactions": m.tool("interactions"),
    }
    m.tool("export_tex", path=f"{name}.tex")
(OUT / "ausgaben.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
m.p.stdin.close()
m.p.wait()
print("ok", list(res))
