"""Select SigmaHQ file rules usable on NTFS metadata: only TargetFilename/SourceFilename/CreationUtcTime."""
import pathlib, shutil, sys, yaml

ALLOWED = {"TargetFilename", "SourceFilename", "CreationUtcTime"}
CATEGORIES = {"file_event", "file_delete", "file_rename", "file_change"}
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
shutil.rmtree(dst, ignore_errors=True)
dst.mkdir(parents=True)

def fields(node, out):
    if isinstance(node, dict):
        for k, v in node.items():
            out.add(k.split("|")[0])
            fields(v, out) if isinstance(v, dict) else None
    elif isinstance(node, list):
        for item in node:
            fields(item, out)

kept, skipped = 0, {}
for p in sorted(src.rglob("*.yml")):
    docs = list(yaml.safe_load_all(p.read_text()))
    rule = docs[0]
    cat = (rule.get("logsource") or {}).get("category")
    if len(docs) != 1 or cat not in CATEGORIES:
        skipped["category/multi-doc"] = skipped.get("category/multi-doc", 0) + 1
        continue
    used = set()
    for name, sel in (rule.get("detection") or {}).items():
        if name not in ("condition", "timeframe"):
            fields(sel, used)
    extra = used - ALLOWED
    if extra:
        key = "uses " + ",".join(sorted(extra))[:40]
        skipped[key] = skipped.get(key, 0) + 1
        continue
    shutil.copy(p, dst / p.name)
    kept += 1
print("kept", kept)
for k, v in sorted(skipped.items(), key=lambda kv: -kv[1])[:8]:
    print(f"  skipped {v:3d}: {k}")
