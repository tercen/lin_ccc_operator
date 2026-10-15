"""Export the assembled relations of a step's result as the platform test expects them.

    TERCEN_TOKEN=... python dev/export_result.py <workflowId> <stepId> <outDir> <prefix>

Walks the step's `computedRelation`, lists every SimpleRelation in the order the install check
diffs them (`simpleAndReferenceRelations`), and for each writes `<prefix>_out_<k>.csv` (via
`tercenctl data export-csv`, untouched) and its `.schema` sidecar (column names and types, in
the CSV's column order). Prints the schema ids so a second run can be `cmp`ed for reproducibility.
"""
import json, os, subprocess, sys
from tercen.client.factory import TercenClient

uri = os.environ.get("TERCEN_HTTP", "http://127.0.0.1:5402")
token = os.environ["TERCEN_TOKEN"]
wf_id, step_id, out_dir, prefix = sys.argv[1:5]
ctx = os.environ.get("TERCEN_CTX", "studio")

client = TercenClient(uri)
client.userService.tercenClient.token = token
client.httpClient.authorization = token
wf = client.workflowService.get(wf_id)
step = next(s for s in wf.steps if s.id == step_id)
rel = step.computedRelation


def simple_relations(r, out):
    k = type(r).__name__
    if k == "SimpleRelation":
        out.append(r.id)
    elif k == "CompositeRelation":
        simple_relations(r.mainRelation, out)
        for jo in r.joinOperators:
            simple_relations(jo.rightRelation, out)
    elif k == "ReferenceRelation":
        out.append(r.relation.id)
    elif hasattr(r, "relation"):
        simple_relations(r.relation, out)
    return out


ids = simple_relations(rel, [])
print("relations", ids)
os.makedirs(out_dir, exist_ok=True)
for k, sid in enumerate(ids, start=1):
    csv_path = os.path.join(out_dir, f"{prefix}_out_{k}.csv")
    subprocess.run(
        ["tercenctl", "--context", ctx, "data", "export-csv", "-s", sid, "--filePath", csv_path, "-q"],
        check=True,
    )
    schema = client.tableSchemaService.get(sid)
    with open(csv_path) as f:
        header = [h.strip().strip('"') for h in f.readline().rstrip("\n").split(",")]
    types = {c.name: c.type for c in schema.columns}
    sidecar = {
        "kind": "TableSchema",
        "columns": [{"kind": "ColumnSchema", "name": h, "type": types[h]} for h in header],
    }
    with open(csv_path + ".schema", "w") as f:
        json.dump(sidecar, f, indent=2)
        f.write("\n")
    n = sum(1 for _ in open(csv_path)) - 1
    print(f"{csv_path}: {n} rows, columns {header}")
