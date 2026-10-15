"""Build the dev workflow: the correlation operator's crabs fixture, projected as tests/test.json.

    tercenctl data upload-csv -p <project> --filePath tests/test_1_in.csv -n crabs
    TERCEN_TOKEN=... python dev/setup_studio.py <workflowId> <tableSchemaId> [prop=value ...]

Leaves a DataStep whose `model.taskId` points at a done CubeQueryTask and whose `model.axis`
holds an XYAxis, which is what `DevContext::from_workflow_step` needs (create-rust-operator §8a).
"""
import os, sys, uuid, json
import tercen.model.impl as m
from tercen.client.factory import TercenClient

uri = os.environ.get("TERCEN_HTTP", "http://127.0.0.1:5402")
token = os.environ["TERCEN_TOKEN"]
wf_id, schema_id = sys.argv[1], sys.argv[2]
props = dict(a.split("=", 1) for a in sys.argv[3:])

client = TercenClient(uri)
client.userService.tercenClient.token = token
client.httpClient.authorization = token
wf = client.workflowService.get(wf_id)

ROW = os.environ.get("DEV_ROW", "sp")
COL = os.environ.get("DEV_COL", "variable")
Y = os.environ.get("DEV_Y", "y_values")
X = os.environ.get("DEV_X", "x_values")
COL_TYPE = os.environ.get("DEV_COL_TYPE", "string")
ROW_TYPE = os.environ.get("DEV_ROW_TYPE", "string")
COL2 = os.environ.get("DEV_COL2", "")  # a second column factor, e.g. the sample

def rect(x, y, w=200.0, h=55.0):
    r = m.Rectangle(); r.topLeft = m.Point(); r.topLeft.x = x; r.topLeft.y = y
    r.extent = m.Point(); r.extent.x = w; r.extent.y = h; return r

def factor(name, typ):
    f = m.Factor(); f.name = name; f.type = typ; return f

def gf(name, typ):
    g = m.GraphicalFactor(); g.factor = factor(name, typ)
    g.rectangle = rect(0.0, 0.0, max(len(name) * 10.0, 30.0), 30.0); return g

def ctable(factors):
    t = m.CrosstabTable(); t.cellSize = 250.0; t.offset = 0; t.nRows = 0
    t.graphicalFactors = factors; t.rectangleSelections = []; return t

# --- TableStep over the uploaded schema -------------------------------------------------------
ts = m.TableStep(); ts.id = str(uuid.uuid4()); ts.name = os.environ.get("DEV_TABLE_NAME", "crabs"); ts.groupId = ""; ts.description = ""
op = m.OutputPort(); op.id = str(uuid.uuid4()); op.name = "table"; op.linkType = "relation"
ts.inputs = []; ts.outputs = [op]; ts.rectangle = rect(100.0, 100.0)
ts.state = m.StepState(); ts.state.taskId = ""; ts.state.taskState = m.DoneState()
rel = m.SimpleRelation(); rel.id = schema_id
ts.model = m.TableStepModel(); ts.model.relation = rel; ts.model.filterSelector = ""

# --- DataStep: rows = variable, cols = observation, y = measurement ---------------------------
ds = m.DataStep(); ds.id = str(uuid.uuid4()); ds.name = "Lin CCC (Rust, dev)"; ds.groupId = ""
ds.description = ""; ds.parentDataStepId = ""
ip = m.InputPort(); ip.id = str(uuid.uuid4()); ip.name = "data"; ip.linkType = "relation"
dop = m.OutputPort(); dop.id = str(uuid.uuid4()); dop.name = "data"; dop.linkType = "relation"
ds.inputs = [ip]; ds.outputs = [dop]; ds.rectangle = rect(100.0, 250.0)
ds.state = m.StepState(); ds.state.taskId = ""; ds.state.taskState = m.InitState()

ct = m.Crosstab(); ct.taskId = ""
ct.axis = m.XYAxisList(); ct.axis.rectangleSelections = []; ct.axis.xyAxis = []
col_factors = [gf(COL2, "string")] if COL2 else []
col_factors.append(gf(COL, COL_TYPE))
ct.columnTable = ctable(col_factors)
ct.rowTable = ctable([gf(ROW, ROW_TYPE)])
ct.filters = m.Filters(); ct.filters.removeNaN = False; ct.filters.namedFilters = []
settings = m.OperatorSettings(); settings.namespace = "ds0"; settings.environment = []
ref = m.OperatorRef(); ref.name = "lin_ccc_operator"; ref.version = "dev"
ref.operatorId = ""; ref.operatorKind = ""
ref.url = m.Url(); ref.url.uri = "https://github.com/tercen/lin_ccc_operator"
ref.propertyValues = []
for k, v in props.items():
    pv = m.PropertyValue(); pv.name = k; pv.value = v; ref.propertyValues.append(pv)
ref.operatorSpec = m.OperatorSpec(); ref.operatorSpec.inputSpecs = []; ref.operatorSpec.outputSpecs = []
settings.operatorRef = ref
ct.operatorSettings = settings
ds.model = ct

link = m.Link(); link.id = str(uuid.uuid4()); link.inputId = ip.id; link.outputId = op.id
wf.steps = list(wf.steps or []) + [ts, ds]
wf.links = list(wf.links or []) + [link]
wf = client.workflowService.update(wf)
print("TABLE_STEP", ts.id)
print("DATA_STEP", ds.id)

# --- CubeQueryTask: what the UI runs so the step has a projection ------------------------------
wf = client.workflowService.get(wf_id)
ds = next(s for s in wf.steps if s.id == ds.id)
q = m.CubeQuery()
q.relation = ts.model.relation
q.colColumns = ([factor(COL2, "string")] if COL2 else []) + [factor(COL, COL_TYPE)]
q.rowColumns = [factor(ROW, ROW_TYPE)]
aq = m.CubeAxisQuery(); aq.chartType = "point"; aq.pointSize = 4
aq.xAxis = factor(X, "double")
aq.yAxis = factor(Y, "double")
aq.colors = []; aq.errors = []; aq.labels = []; aq.preprocessors = []
aq.xAxisSettings = m.AxisSettings(); aq.xAxisSettings.meta = []
aq.yAxisSettings = m.AxisSettings(); aq.yAxisSettings.meta = []
q.axisQueries = [aq]
q.filters = ds.model.filters
q.operatorSettings = ds.model.operatorSettings
task = m.CubeQueryTask(); task.state = m.InitState(); task.owner = wf.acl.owner
task.projectId = wf.projectId; task.query = q
task = client.taskService.create(task)
client.taskService.runTask(task.id)
task = client.taskService.waitDone(task.id)
state = type(task.state).__name__
print("CUBE_QUERY_TASK", task.id, state, getattr(task.state, "reason", ""))
if state != "DoneState":
    sys.exit(1)

# --- point the step at it, and give it the XYAxis DevContext requires --------------------------
axis = json.loads('{"kind":"XYAxisList","xyAxis":[{"kind":"XYAxis","chart":{"kind":"ChartPoint","name":"","pointSize":4,"properties":{"kind":"Properties","properties":[],"propertyValues":[]}},"xAxis":{"kind":"Axis","axisExtent":{"x":80.0,"y":30.0,"kind":"Point"},"axisSettings":{"kind":"AxisSettings","meta":[]},"graphicalFactor":{"kind":"GraphicalFactor","factor":{"kind":"Factor","name":"XMEASUREMENT","type":"double"},"rectangle":{"kind":"Rectangle","extent":{"x":0.0,"y":0.0,"kind":"Point"},"topLeft":{"x":0.0,"y":0.0,"kind":"Point"}}}},"yAxis":{"kind":"Axis","axisExtent":{"x":80.0,"y":30.0,"kind":"Point"},"axisSettings":{"kind":"AxisSettings","meta":[]},"graphicalFactor":{"kind":"GraphicalFactor","factor":{"kind":"Factor","name":"MEASUREMENT","type":"double"},"rectangle":{"kind":"Rectangle","extent":{"x":0.0,"y":0.0,"kind":"Point"},"topLeft":{"x":0.0,"y":0.0,"kind":"Point"}}}},"colors":{"kind":"Colors","factors":[],"palette":{"kind":"CategoryPalette","backcolor":0,"colorList":{"kind":"ColorList","name":""},"properties":[],"stringColorElements":[]}},"errors":{"kind":"Errors","factors":[]},"labels":{"kind":"Labels","factors":[]},"taskId":"","preprocessors":[]}],"rectangleSelections":[]}'.replace("XMEASUREMENT", X).replace("MEASUREMENT", Y))
axis["xyAxis"][0]["taskId"] = task.id
wf = client.workflowService.get(wf_id)
ds = next(s for s in wf.steps if s.id == ds.id)
ds.model.taskId = task.id
ds.model.axis = m.XYAxisList(axis)
client.workflowService.update(wf)
print("READY workflow", wf_id, "step", ds.id)
