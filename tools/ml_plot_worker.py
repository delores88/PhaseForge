"""Trusted data-only ML evidence plot. Pillow plus Python standard library only.

No model fitting, solver execution, arbitrary code, network access or OOD prediction.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 1600, 1000
ROLE_COLORS = {'train':'#7296bb', 'validation':'#b49ae8', 'calibration':'#72ceba', 'test':'#62b7ff', 'regime_test':'#f8bd65', 'ood':'#fc7c87'}
GATE_LABELS = {'accuracy':'Accuracy', 'baseline_improvement':'Baseline improvement', 'coverage_and_support':'Coverage / support', 'warm_speed':'Warm speed', 'total_workload_cost':'Total workload cost'}
T95 = 4.302653

def require(condition, message):
    if not condition:
        raise ValueError(message)

def finite(value, name, positive=False):
    require(type(value) in (int, float) and math.isfinite(value), f'{name} must be finite')
    require(abs(value) <= 1e12 and (not positive or value > 0), f'{name} is outside plotting bounds')
    return float(value)

def digest(value, name):
    require(isinstance(value,str) and len(value)==64 and all(c in '0123456789abcdef' for c in value), f'{name} must be a lowercase SHA-256 digest')
    return value

def close(actual, expected, name):
    require(abs(actual-expected) <= max(1, abs(actual), abs(expected))*1e-8, f'{name} disagrees with retained data')

def checked_json(raw):
    def invalid(value): raise ValueError(f'Non-finite JSON constant {value}')
    value=json.loads(raw, parse_constant=invalid)
    def walk(item, depth=0):
        require(depth<=32, 'JSON nesting exceeds 32 levels')
        if isinstance(item,float): require(math.isfinite(item), 'Non-finite JSON value')
        elif isinstance(item,dict):
            for child in item.values():walk(child,depth+1)
        elif isinstance(item,list):
            for child in item:walk(child,depth+1)
    walk(value)
    return value

def read_pin(spec, base):
    require(isinstance(spec,dict) and set(spec)=={'path','sha256'}, 'Source pin requires exactly path and sha256')
    expected=digest(spec['sha256'],'Source hash')
    require(isinstance(spec['path'],str) and '://' not in spec['path'] and '\x00' not in spec['path'], 'Source must be a local file')
    path=Path(spec['path']);path=path if path.is_absolute() else base/path
    require(path.is_file() and not path.is_symlink(), 'Source must be an existing regular file')
    require(path.stat().st_size<=4*1024*1024, 'Source exceeds 4 MiB')
    raw=path.read_bytes()
    require(len(raw)<=4*1024*1024, 'Source exceeds 4 MiB')
    require(hashlib.sha256(raw).hexdigest()==expected, 'Source SHA-256 mismatch')
    return checked_json(raw), {'sha256':expected,'bytes':len(raw)}

def summary(seeds, condition, role):
    require(isinstance(seeds,list) and len(seeds)==3,'Each endpoint needs exactly three independent seed rows')
    require({s.get('replicate_index') for s in seeds}=={0,1,2}, 'Seed replicate indices must be 0,1,2')
    require(len({s.get('seed') for s in seeds})==3 and all(type(s.get('seed')) is int for s in seeds), 'Seed identifiers must be distinct integers')
    require(all(type(s['replicate_index']) is int and s['seed']==110000+1000*condition['condition_index']+s['replicate_index'] for s in seeds),'Seed differs from the preregistered replicate identity')
    require(all(isinstance(s.get('solver_job_id'),str) and s['solver_job_id'] for s in seeds),'Each seed needs its solver job identity')
    require(len({s['solver_job_id'] for s in seeds})==3, 'Seed rows must refer to distinct solver jobs')
    for s in seeds:
        for key in ('condition_index','temperature_kelvin','density_g_cm3'):
            require(s.get(key)==condition[key],f'Seed {key} differs from the frozen condition')
        require(s.get('role')==role,'Seed role differs from its frozen condition')
    values=[finite(s['pressure_bar'],'Seed pressure') for s in seeds]
    mean=statistics.mean(values);sd=statistics.stdev(values);se=sd/math.sqrt(3)
    return {'mean_bar':mean,'seed_sd_bar':sd,'seed_se_bar':se,'seed_t95_bar':[mean-T95*se,mean+T95*se],'seeds':seeds}

def validate(config, base):
    require(isinstance(config,dict) and set(config)=={'schema_version','evaluation','split','ood'} and config['schema_version']==1,'Plot input requires schema_version 1 and three source pins')
    evaluation, ep=read_pin(config['evaluation'],base);split,sp=read_pin(config['split'],base);ood,op=read_pin(config['ood'],base)
    require(all(v.get('schema_version')==1 for v in (evaluation,split,ood)),'Evidence schema version must be 1')
    study=split.get('study_id');require(isinstance(study,str) and 0<len(study)<=160,'Missing bounded study identity')
    for value in (evaluation,ood):
        require(value.get('study_id')==study,'Sources belong to different studies')
        for key in ('proposal_sha256','protocol_sha256'):
            require(digest(value.get(key),key)==digest(split.get(key),key),'Source proposal or protocol identity mismatch')
        require(value.get('split_sha256')==sp['sha256'],'Source split digest differs from supplied split bytes')
    digest(evaluation.get('model_sha256'),'Model hash');digest(evaluation.get('calibration_sha256'),'Calibration hash')
    protocol=split.get('protocol',{})
    require(hashlib.sha256(json.dumps(protocol,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False).encode('ascii')).hexdigest()==split['protocol_sha256'],'Protocol digest does not match its declared fields')
    require(protocol.get('engine')=='openmm_argon','Plot supports the frozen argon pressure study only')
    require(protocol.get('target',{}).get('replicates')==3 and protocol['target'].get('name')=='mean_pressure_bar','Unexpected pressure target or replicate count')
    require(protocol.get('feature_schema')==[{'bounds':[220,430],'name':'temperature_kelvin','unit':'K'},{'bounds':[.25,.85],'name':'density_g_cm3','unit':'g/cm^3'}],'Feature units or frozen bounds changed')
    conditions=split.get('conditions');require(isinstance(conditions,list) and len(conditions)==32,'Split requires exactly 32 fixed conditions')
    require({c.get('condition_index') for c in conditions}==set(range(32)),'Frozen condition IDs must be 0 through 31')
    lookup={c['condition_index']:c for c in conditions}
    for i,c in lookup.items():
        expected_t=[220,250,280,310,325,370,400,430][i//4];expected_rho=[.25,.45,.65,.85][i%4]
        require(c.get('temperature_kelvin')==expected_t and c.get('density_g_cm3')==expected_rho,'Frozen condition grid changed')
        require(c.get('role') in ROLE_COLORS and c['role']!='ood','Unexpected split role')
        key=f'phaseforge-ml-argon-v1|T={expected_t:g}|rho={expected_rho:.2f}'
        require(c.get('split_key')==key and c.get('split_sha256')==hashlib.sha256(key.encode()).hexdigest(),'Condition membership digest changed')
    ordered=sorted((c for c in conditions if c['temperature_kelvin']!=325),key=lambda c:(c['split_sha256'],c['temperature_kelvin'],c['density_g_cm3']))
    for i,c in enumerate(ordered):
        expected_role='train' if i<12 else 'validation' if i<15 else 'calibration' if i<24 else 'test'
        require(c['role']==expected_role,'Frozen condition role assignment changed')
    require(all(c['role']=='regime_test' for c in conditions if c['temperature_kelvin']==325),'Regime holdout role changed')
    require(split.get('ood')=={'condition_index':32,'temperature_kelvin':180,'density_g_cm3':.55,'role':'ood'},'Unexpected OOD condition')
    require(ood.get('role')=='ood','OOD shard role changed')
    ood_summary=summary(ood.get('seeds'),split['ood'],'ood')
    require(evaluation.get('held_out_evaluation_complete') is True and evaluation.get('held_out_count')==8,'Held-out evaluation must be complete with eight conditions')
    cases=evaluation.get('cases');require(isinstance(cases,list) and len(cases)==8,'Exactly eight evaluated cases required')
    expected_ids={c['condition_index'] for c in conditions if c['role'] in ('test','regime_test')}
    require(len(expected_ids)==8 and {c.get('condition_index') for c in cases}==expected_ids,'Held-out IDs do not match the frozen split')
    require(sum(c['role']=='test' for c in conditions)==4 and sum(c['role']=='regime_test' for c in conditions)==4,'Expected four test and four regime-test conditions')
    q=finite(evaluation.get('q_Z'),'Calibrated interval radius');require(q>=0,'Calibrated interval radius must be nonnegative')
    for c in cases:
        condition=lookup[c['condition_index']]
        for key in ('role','temperature_kelvin','density_g_cm3','split_key','split_sha256'):
            require(c.get(key)==condition[key],f'Evaluation {key} changed')
        s=summary(c.get('seeds'),condition,c['role'])
        for key,expected in [('pressure_bar',s['mean_bar']),('seed_sd_bar',s['seed_sd_bar']),('seed_se_bar',s['seed_se_bar'])]:close(finite(c.get(key),key),expected,key)
        for key in ('seed_t95_bar','interval_bar'):
            require(isinstance(c.get(key),list) and len(c[key])==2,f'{key} needs two endpoints')
            values=[finite(v,key) for v in c[key]];require(values[0]<=values[1],f'{key} endpoints reversed')
        for a,b in zip(c['seed_t95_bar'],s['seed_t95_bar']):close(a,b,'Seed t95 interval')
        pred=finite(c.get('prediction_bar'),'Prediction');p0=finite(c.get('p0_bar'),'Pressure normalization',True)
        close(pred,finite(c.get('prediction_Z'),'Normalized prediction')*p0,'Prediction normalization')
        for a,b in zip(c['interval_bar'],[pred-q*p0,pred+q*p0]):close(a,b,'Calibrated interval')
        require(type(c.get('covered')) is bool and type(c.get('support_admitted')) is bool,'Missing coverage/support flags')
        finite(c.get('support_distance'),'Support distance')
    gates=evaluation.get('gates');require(isinstance(gates,dict) and set(gates)==set(GATE_LABELS) and all(type(v) is bool for v in gates.values()),'Missing or malformed evaluation gates')
    require(type(evaluation.get('useful_acceleration')) is bool and evaluation['useful_acceleration']==all(gates.values()),'Overall gate status contradicts individual gates')
    require(set(evaluation.get('rejections',[]))=={k for k,v in gates.items() if not v},'Gate rejection list contradicts saved evaluation')
    require(evaluation.get('coverage_count')==sum(c['covered'] for c in cases),'Coverage count contradicts cases')
    require(evaluation.get('support_count')==sum(c['support_admitted'] for c in cases),'Support count contradicts cases')
    synthetic=any(s.get('fixture_only') or 'synthetic' in s['solver_job_id'].lower() for c in cases for s in c['seeds']) or any(s.get('fixture_only') or 'synthetic' in s['solver_job_id'].lower() for s in ood['seeds'])
    return evaluation,split,ood_summary,{'evaluation':ep,'split':sp,'ood':op},synthetic

def font(size,bold=False):
    candidates=[Path(os.environ.get('WINDIR','C:/Windows'))/'Fonts'/('segoeuib.ttf' if bold else 'segoeui.ttf'),Path('/usr/share/fonts/truetype/dejavu')/('DejaVuSans-Bold.ttf' if bold else 'DejaVuSans.ttf')]
    for path in candidates:
        if path.is_file():return ImageFont.truetype(str(path),size)
    return ImageFont.load_default(size=size)

def mapper(domain, rectangle):
    xmin,xmax,ymin,ymax=domain;left,top,right,bottom=rectangle
    return lambda x,y:[left+(x-xmin)*(right-left)/(xmax-xmin),bottom-(y-ymin)*(bottom-top)/(ymax-ymin)]

def render(config,base,output):
    evaluation,split,ood,pins,synthetic=validate(config,base)
    image=Image.new('RGB',(WIDTH,HEIGHT),'#091322');draw=ImageDraw.Draw(image)
    white='#e5edf8';muted='#a8bad0';grid='#27394d'
    def text(x,y,value,size=18,fill=white,bold=False,anchor=None):draw.text((x,y),str(value),font=font(size,bold),fill=fill,anchor=anchor)
    title='Argon pressure surrogate | retained numerical evidence'
    text(48,25,title,32,bold=True)
    text(48,73,'SYNTHETIC COMPONENT FIXTURE — not a scientific result' if synthetic else 'Finite-window solver endpoints · frozen model · three independent seeds per condition',19,'#f8bd65' if synthetic else muted)
    status='PASSED' if evaluation['useful_acceleration'] else 'REJECTED'
    draw.rounded_rectangle((48,117,1552,171),radius=10,fill='#173830' if status=='PASSED' else '#3a2632')
    text(65,128,f'Useful-acceleration gate: {status}',22,bold=True)
    text(670,133,f"Coverage {evaluation['coverage_count']}/8 · support {evaluation['support_count']}/8 · cost review {'complete' if evaluation.get('cost_evaluation_complete') else 'incomplete'}",18)
    draw.rounded_rectangle((48,192,792,879),radius=14,fill='#111f30')
    draw.rounded_rectangle((812,192,1552,879),radius=14,fill='#111f30')
    text(70,211,'Held-out pressure',24,bold=True);text(834,211,'Frozen condition coverage',24,bold=True)
    left_rect=[124,300,750,685]
    extent=[v for c in evaluation['cases'] for v in [c['pressure_bar'],c['prediction_bar'],*c['seed_t95_bar'],*c['interval_bar']]]
    lo,hi=min(extent),max(extent);padding=max(1,(hi-lo)*.10);lo-=padding;hi+=padding
    left_domain=[lo,hi,lo,hi];project=mapper(left_domain,left_rect)
    text(70,246,'Horizontal: 3-seed mean ± t95 · vertical: calibrated interval',16,muted)
    for i in range(6):
        value=lo+(hi-lo)*i/5;x,y=project(value,value)
        draw.line((x,left_rect[1],x,left_rect[3]),fill=grid,width=1);draw.line((left_rect[0],y,left_rect[2],y),fill=grid,width=1)
        text(x,695,f'{value:.4g}',15,muted,anchor='mt');text(113,y,f'{value:.4g}',15,muted,anchor='rm')
    draw.rectangle(left_rect,outline='#536981',width=1)
    parity=[project(lo,lo),project(hi,hi)];draw.line([tuple(p) for p in parity],fill='#8094aa',width=2)
    text(437,729,'Measured pressure (bar)',18,anchor='mt');text(72,276,'Predicted pressure (bar)',16,muted)
    text(137,311,'Parity: prediction = measurement',15,muted)
    points=[];label_boxes=[]
    centers=[project(c['pressure_bar'],c['prediction_bar']) for c in evaluation['cases']]
    for c in sorted(evaluation['cases'],key=lambda row:row['condition_index']):
        x,y=project(c['pressure_bar'],c['prediction_bar']);fill=ROLE_COLORS[c['role']]
        horizontal=[project(v,c['prediction_bar']) for v in c['seed_t95_bar']]
        vertical=[project(c['pressure_bar'],v) for v in c['interval_bar']]
        for ends in (horizontal,vertical):draw.line([tuple(p) for p in ends],fill=fill,width=3)
        for ex,ey in horizontal:draw.line((ex,ey-5,ex,ey+5),fill=fill,width=2)
        for ex,ey in vertical:draw.line((ex-5,ey,ex+5,ey),fill=fill,width=2)
        draw.ellipse((x-5,y-5,x+5,y+5),fill=fill,outline='#f2f6fc',width=1)
        label=f"#{c['condition_index']}";label_font=font(16,True);label_width=draw.textlength(label,font=label_font)
        def overlaps(a,b):return a[0]<b[2]+3 and a[2]+3>b[0] and a[1]<b[3]+3 and a[3]+3>b[1]
        label_box=None
        for dy in (-24,12,-46,34,-68,56,-90,78):
            for dx in (11,-label_width-11):
                candidate=list(draw.textbbox((x+dx,y+dy),label,font=label_font))
                if candidate[0]<left_rect[0]+4 or candidate[2]>left_rect[2]-4 or candidate[1]<left_rect[1]+4 or candidate[3]>left_rect[3]-4:continue
                if any(overlaps(candidate,box) for box in label_boxes) or any(overlaps(candidate,[cx-6,cy-6,cx+6,cy+6]) for cx,cy in centers):continue
                label_box=candidate;label_position=(x+dx,y+dy);break
            if label_box:break
        if label_box is None:
            label_position=(max(left_rect[0]+4,min(x+11,left_rect[2]-label_width-4)),max(left_rect[1]+4,min(y+12,left_rect[3]-24)))
            label_box=list(draw.textbbox(label_position,label,font=label_font))
        label_boxes.append(label_box)
        if abs(label_position[1]-y)>30:draw.line((x,y,label_box[0] if label_box[0]>x else label_box[2],(label_box[1]+label_box[3])/2),fill=fill,width=1)
        draw.text(label_position,label,font=label_font,fill=fill)
        points.append({'condition_index':c['condition_index'],'role':c['role'],'color':fill,'pressure_bar':c['pressure_bar'],'prediction_bar':c['prediction_bar'],'seed_t95_bar':c['seed_t95_bar'],'interval_bar':c['interval_bar'],'center_px':[x,y],'horizontal_endpoints_px':horizontal,'vertical_endpoints_px':vertical,'label_rect_px':label_box,'support_admitted':c['support_admitted'],'covered':c['covered']})
    text(71,772,'Gates from the saved evaluation',17,bold=True)
    for i,(key,label) in enumerate(GATE_LABELS.items()):
        x=71+(i%2)*352;y=803+(i//2)*22
        text(x,y,f"{'PASS' if evaluation['gates'][key] else 'FAIL'}  {label}",15,'#86d8b4' if evaluation['gates'][key] else '#ff9baa')
    right_rect=[906,300,1511,607];right_domain=[165,445,.18,.92];coverage_project=mapper(right_domain,right_rect)
    text(834,246,'All 32 split conditions + completed direct-solver fallback',16,muted)
    for value in [180,220,250,280,310,325,370,400,430]:
        x,_=coverage_project(value,.18);draw.line((x,300,x,607),fill=grid,width=1)
        text(x,617,str(value),14,muted,anchor='mt')
    for value in [.25,.45,.55,.65,.85]:
        _,y=coverage_project(165,value);draw.line((906,y,1511,y),fill=grid,width=1);text(895,y,f'{value:.2f}',15,muted,anchor='rm')
    draw.rectangle(right_rect,outline='#536981',width=1)
    text(1208,648,'Temperature (K)',18,anchor='mt');text(834,276,'Density (g/cm³)',16,muted)
    coverage=[]
    for c in sorted(split['conditions'],key=lambda row:row['condition_index']):
        x,y=coverage_project(c['temperature_kelvin'],c['density_g_cm3']);fill=ROLE_COLORS[c['role']]
        draw.ellipse((x-11,y-11,x+11,y+11),fill=fill,outline=white,width=1);text(x,y,str(c['condition_index']),12,'#07121d',True,anchor='mm')
        coverage.append({**c,'center_px':[x,y],'color':fill})
    ox,oy=coverage_project(180,.55);draw.polygon([(ox,oy-14),(ox+14,oy),(ox,oy+14),(ox-14,oy)],fill=ROLE_COLORS['ood'],outline=white,width=2)
    text(ox+21,oy-10,'OOD',15,ROLE_COLORS['ood'],True)
    for i,(role,label) in enumerate([('train','Train'),('validation','Validation'),('calibration','Calibration'),('test','Test'),('regime_test','Regime test')]):
        x=834+i*139;draw.ellipse((x,689,x+10,699),fill=ROLE_COLORS[role]);text(x+16,684,label,14,muted)
    text(834,719,'OOD: 180 K / 0.55 g/cm³ — measured fallback only',18,bold=True)
    text(834,750,f"Pressure mean = {ood['mean_bar']:.6g} bar (3 seeds)",18)
    text(834,778,f"Seed SD = {ood['seed_sd_bar']:.5g} bar · SE = {ood['seed_se_bar']:.5g} bar",17,muted)
    text(834,808,'Outside trained temperature range. No OOD prediction plotted.',16,ROLE_COLORS['ood'])
    text(834,837,'Condition numbers are frozen IDs, not additional replicates.',15,muted)
    text(48,901,'Seed t95 describes mean uncertainty across 3 runs (df=2). Calibrated intervals are model residual bounds, not seed SD.',17,muted)
    text(48,928,'This finite computational protocol does not establish population coverage, equilibrium pressure, or experimental validity.',17,muted)
    text(48,964,f"Model {evaluation['model_sha256'][:16]} · evaluation {pins['evaluation']['sha256'][:16]} · split {pins['split']['sha256'][:16]} · OOD {pins['ood']['sha256'][:16]}",15,'#8299b4')
    output.mkdir(parents=True,exist_ok=True)
    image.save(output/'plot.png')
    report={'schema_version':1,'width':WIDTH,'height':HEIGHT,'study_id':split['study_id'],'model_sha256':evaluation['model_sha256'],'proposal_sha256':split['proposal_sha256'],'protocol_sha256':split['protocol_sha256'],'source_hashes':pins,'synthetic_fixture':synthetic,'scientific_rerun':False,'useful_acceleration':evaluation['useful_acceleration'],'gates':evaluation['gates'],'rejections':evaluation['rejections'],'uncertainty':{'horizontal':'Mean +/- 4.302653*SE across 3 independent seeds; df=2, two-sided Student t95','vertical':'Frozen calibrated model residual interval; not seed SD or experimental uncertainty'},'held_out':{'axis_domain':left_domain,'plot_rect':left_rect,'x_unit':'bar','y_unit':'bar','parity_line_px':parity,'points':points},'coverage':{'axis_domain':right_domain,'plot_rect':right_rect,'x_unit':'K','y_unit':'g/cm^3','points':coverage,'ood':{'condition_index':32,'temperature_kelvin':180,'density_g_cm3':.55,'center_px':[ox,oy],'color':ROLE_COLORS['ood'],**ood,'prediction':None}},'scope':'Saved numerical evaluation only; no scientific or experimental validity beyond the supplied protocol'}
    (output/'plot.json').write_text(json.dumps(report,indent=2,allow_nan=False),encoding='utf-8')
    artifacts=[]
    for name in ('plot.png','plot.json'):
        blob=(output/name).read_bytes();artifacts.append({'path':name,'bytes':len(blob),'sha256':hashlib.sha256(blob).hexdigest()})
    result={'schema_version':1,'state':'completed','width':WIDTH,'height':HEIGHT,'artifacts':artifacts,'scientific_rerun':False,'synthetic_fixture':synthetic,'source_hashes':pins,'model_sha256':evaluation['model_sha256'],'study_id':split['study_id']}
    (output/'result.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    return result

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--input',required=True);parser.add_argument('--output',required=True);args=parser.parse_args()
    config_path=Path(args.input).resolve();require(config_path.stat().st_size<=64*1024,'Plot config exceeds 64 KiB')
    result=render(checked_json(config_path.read_bytes()),config_path.parent,Path(args.output).resolve())
    print(json.dumps({'state':result['state'],'artifacts':result['artifacts'],'synthetic_fixture':result['synthetic_fixture']}))

if __name__=='__main__':main()
