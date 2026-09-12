"""Small actual Blender consumer check of retained continuum fields; no solver.

Uses an existing Blender executable. Two-frame clips consume the original
numeric endpoints, then Blender's decoder confirms dimensions/frame counts.
All source artifacts are checked unchanged and all emitted source pins verified.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def read(path):return json.loads(path.read_text(encoding="utf-8"))
def write(path,value):path.write_text(json.dumps(value,indent=2,sort_keys=True,allow_nan=False),encoding="utf-8")
def snapshot(root):return {p.relative_to(root).as_posix():sha(p) for p in root.rglob("*") if p.is_file()}


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--blender",type=Path,required=True);parser.add_argument("--sources",type=Path,required=True);parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
    repo=Path(__file__).resolve().parents[1];sources=args.sources.resolve();blender=args.blender.resolve(strict=True)
    report={"scope":"Source-level actual Blender consumption of retained numerical data; not installed/native UI acceptance","passed":False,"blender_sha256":sha(blender),"checker_sha256":sha(Path(__file__)),"cases":[]}
    try:
        for name in ("heat-refinement-32","flow-taylor-green"):
            source=sources/name;before=snapshot(source);index=read(source/"fields/index.json");directory=root/name;directory.mkdir()
            for filename in ("field_render.py","trajectory_render.py"):
                shutil.copyfile(repo/"tools"/filename,directory/filename)
            request={"source_directory":str(source),"mode":"video","width":640,"height":360,"fps":2,"playback_duration_seconds":1,"frame_count":2,
                     "start_time":index["frames"][0]["time"],"end_time":index["frames"][-1]["time"],"renderer":"eevee","samples":8,"labels":True,"presentation":{}}
            write(directory/"input.json",request);started=time.perf_counter()
            with (directory/"stdout.log").open("wb") as out,(directory/"stderr.log").open("wb") as err:
                process=subprocess.run([str(blender),"--background","--factory-startup","--disable-autoexec","--threads","1","--python-exit-code","1","--python",str(directory/"field_render.py"),"--","--input",str(directory/"input.json"),"--output",str(directory)],stdout=out,stderr=err,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0)
            assert process.returncode==0,(name,process.returncode,(directory/"stderr.log").read_text())
            receipt=read(directory/"result.json")
            assert receipt["state"]=="completed" and receipt["scientific_rerun"] is False
            assert receipt["representation"]=="scalar_field" and receipt["time_unit"]=="s"
            assert receipt["source_index_sha256"]==before["fields/index.json"]
            assert receipt["start_time"]==request["start_time"] and receipt["end_time"]==request["end_time"]
            assert receipt["decode_check"]["width"]==640 and receipt["decode_check"]["height"]==360 and receipt["decode_check"]["frame_count"]==2
            assert receipt["sha256"]==sha(directory/"simulation.mp4")
            for relative,digest in receipt["source_fields"].items():assert digest==before[relative]
            for position,frame in zip((0,-1),receipt["endpoint_frames"]):
                original=index["frames"][position]
                assert frame["step"]==original["step"] and frame["display_time"]==original["time"] and frame["field_sha256"]==original["sha256"] and frame["interpolated"] is False
            assert snapshot(source)==before,"Renderer modified numerical source artifacts"
            assert sha(directory/"first-frame.png")!=sha(directory/"last-frame.png"),"Distinct computed endpoints must yield different observed images"
            report["cases"].append({"name":name,"engine":read(source/"manifest.json")["engine"],"field_name":index["field_name"],"field_unit":index["field_unit"],"length_unit":index["length_unit"],
                                    "source_index_sha256":receipt["source_index_sha256"],"physical_start_s":receipt["start_time"],"physical_end_s":receipt["end_time"],"wall_seconds":time.perf_counter()-started,
                                    "video_sha256":receipt["sha256"],"first_png_sha256":sha(directory/"first-frame.png"),"last_png_sha256":sha(directory/"last-frame.png"),"decoded":receipt["decode_check"],"source_unchanged":True,"scientific_rerun":False})
            write(root/"report.json",report)
        report["passed"]=True
    except Exception as error:
        report["error"]={"type":type(error).__name__,"message":str(error)}
        raise
    finally:
        write(root/"report.json",report);print(json.dumps(report,sort_keys=True))


if __name__=="__main__":main()
