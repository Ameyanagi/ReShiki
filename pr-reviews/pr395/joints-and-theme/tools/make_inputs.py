"""Write the 16 evidence inputs: caffeine and ibuprofen x Stick / Ball & stick x
light / dark canvas x Publication / Jmol theme, all Flat look, from the phase-2
evidence files (ReShiki's own MMFF94s conformer and opening view).

usage: make_inputs.py <ev386-2 dir> <out input dir>
"""
import json, sys, os
SRC, OUT = sys.argv[1], sys.argv[2]
os.makedirs(OUT, exist_ok=True)
STYLES = {"stick": "stick", "ballstick": "ball_stick"}
for mol in ["caffeine", "ibuprofen"]:
    for tag, style in STYLES.items():
        base = json.load(open(f"{SRC}/input/own/{mol}-{tag}-flat.rsk"))
        assert base["models"][0]["style"] == style
        for canvas in ["light", "dark"]:
            for theme in ["publication", "jmol"]:
                doc = json.loads(json.dumps(base))
                doc.pop("canvas_theme", None); doc.pop("color_theme", None)
                if canvas == "dark": doc["canvas_theme"] = "dark"
                if theme == "jmol": doc["color_theme"] = "jmol"
                json.dump(doc, open(f"{OUT}/{mol}-{tag}-{canvas}-{theme}.rsk", "w"))
