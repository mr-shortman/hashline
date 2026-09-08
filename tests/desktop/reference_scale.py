"""Reference-machine acceptance: temporarily scale all outputs and restore the logical configuration.

Fails before mutation for unsupported scales, rotations or mirrored outputs. Other machines should configure
scale using their desktop settings and run run_acceptance.py directly.
"""
import json
import os
import subprocess
import sys
from pathlib import Path
from gi.repository import Gio, GLib

bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
dest='org.gnome.Mutter.DisplayConfig'
path='/org/gnome/Mutter/DisplayConfig'
def call(method, params=None):
    return bus.call_sync(dest,path,dest,method,params,None,Gio.DBusCallFlags.NONE,-1,None).unpack()
def state(): return call('GetCurrentState')
serial, monitors, logical, props=state()
configs=[]
for x,y,scale,transform,primary,specs,properties in logical:
    entries=[]
    for spec in specs:
        physical=next(m for m in monitors if m[0]==spec)
        mode=next(m[0] for m in physical[1] if m[6].get('is-current'))
        entries.append((spec[0],mode,{}))
    configs.append((x,y,scale,transform,primary,entries))
def apply(values):
    serial=state()[0]
    call('ApplyMonitorsConfig',GLib.Variant('(uua(iiduba(ssa{sv}))a{sv})',(serial,1,values,{'layout-mode':GLib.Variant('u',props['layout-mode'])})))
scale=float(sys.argv[1])
# Scale every output for this run so compositor placement cannot silently put
# the test window on an unscaled secondary monitor. Restore the exact layout.
changed=[]
x=0
for _,y,old,transform,primary,entries in sorted(configs,key=lambda c:not c[4]):
    assert transform==0 and len(entries)==1, 'Unsupported rotated/mirrored reference layout'
    physical=next(m for m in monitors if m[0][0]==entries[0][0])
    mode=next(m for m in physical[1] if m[0]==entries[0][1])
    assert scale in mode[5], 'Requested scale not supported'
    changed.append((x,0,scale,transform,primary,entries))
    x+=round(mode[1]/scale)
record=Path(os.environ.get('HASHLINE_ACCEPTANCE_OUTPUT','test-results/acceptance'))/f'display-{scale}.json'
record.parent.mkdir(parents=True,exist_ok=True)
try:
    apply(changed)
    actual=state()
    record.write_text(json.dumps({'before':logical,'during':actual[2]},indent=2)+'\n')
    subprocess.run(sys.argv[2:],check=True)
finally:
    apply(configs)
    assert state()[2]==logical, 'Monitor restore mismatch'
    value=json.loads(record.read_text())
    value['restored']=True
    record.write_text(json.dumps(value,indent=2)+'\n')
