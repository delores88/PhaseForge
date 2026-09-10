"""Generate the provider-neutral, closed proposal schema. No scientific cases.
Provider schemas constrain representation; Rust validation constrains meaning/budgets.
Run with Python 3, with no dependencies. Output is consumed directly by Rust.
"""
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
def obj(**props):
    return {'type':'object','properties':props,'required':list(props),'additionalProperties':False}
def arr(item): return {'type':'array','items':item}
def enum(*values): return {'type':'string','enum':list(values)}
def ref(name): return {'$ref':f'#/$defs/{name}'}
s={'type':'string'}; n={'type':'number'}; i={'type':'integer'}; b={'type':'boolean'}
d={}
d['distribution']={'anyOf':[
    obj(kind=enum('explicit'),values=arr(arr(n))),
    obj(kind=enum('uniform'),min=arr(n),max=arr(n)),
    obj(kind=enum('normal'),mean=arr(n),std_dev=arr(n)),
    obj(kind=enum('grid'),extent=arr(i),spacing=n,jitter=n),
    obj(kind=enum('sphere'),radius=n,thickness=n),
]}
d['boundary']={'anyOf':[obj(kind=enum('open')),
    obj(kind=enum('periodic'),min=arr(n),max=arr(n)),
    obj(kind=enum('reflective'),min=arr(n),max=arr(n))]}
d['model']={'anyOf':[
    obj(kind=enum('state_vector_ode'),variables=arr(obj(name=s,unit=s,initial=n)),derivatives=arr(obj(variable=s,expression=s))),
    obj(kind=enum('pairwise_particles'),dimensions=i,
        population=obj(count=i,mass=n,position=ref('distribution'),velocity=ref('distribution')),
        interaction=obj(radial_force=s,cutoff={'type':['number','null']},softening=n,linear_damping=n,external_acceleration=arr(s)),
        boundary=ref('boundary'))]}
d['integration']=obj(method=enum('euler','rk4','velocity_verlet'),start_time=n,end_time=n,time_step=n,output_stride=i,max_steps=i)
d['manifest']=obj(title=s,question=s,scientific_boundary=s,hypothesis=s,model=ref('model'),
    constants=arr(obj(name=s,value=n,unit=s,description=s)),
    trajectory=arr(obj(name=s,expression=s,reducer=enum('minimum','maximum','range','time_mean','integral','duration_below','duration_above','entries_below','entries_above','first_below','first_above'),unit=s,threshold=n,hysteresis=n)),integration=ref('integration'),
    search=obj(enabled=b,algorithm=enum('none','random','evolutionary','latin_hypercube','differential_evolution'),
        variables=arr(obj(name=s,target=s,minimum=n,maximum=n)),
        objectives=arr(obj(name=s,expression=s,goal=enum('minimize','maximize'),weight=n)),
        population=i,generations=i,elite_fraction=n,mutation_scale=n,seed=i),
    observables=arr(obj(name=s,expression=s,unit=s)),constraints=arr(obj(name=s,expression=s,tolerance=n)),
    visualization=obj(kind=enum('trajectory','particle_cloud','scatter','none'),x=s,y=s,z=s,point_size=n,max_frames=i,trails=b,entities=arr(obj(id=s,x=s,y=s,z=s,vx=s,vy=s,vz=s,radius=s))),
    falsification=arr(obj(name=s,kind=enum('step_halving','resolution_ladder','seed_replication','initial_perturbation','parameter_perturbation'),target={'type':['string','null']},magnitude=n,repetitions=i,checks=arr(obj(metric=s,expectation=enum('stable','change','decrease','increase'),absolute_tolerance=n,relative_tolerance=n)))),
    compute=obj(preference=enum('auto','cpu','gpu'),policy=enum('interactive','balanced','throughput'),candidate_count=i,batch_size=i,max_wall_seconds=i,max_memory_mb=i),
    limitations=arr(s))
d['research_task']=obj(id=s,title=s,kind=enum('literature','data_analysis','simulation','external_engine','expert_review'),question=s,method=s,
    inputs=arr(s),depends_on=arr(s),success_criterion=s,deliverable=s,next_prompt=s,search_query=s)
d['research_plan']=obj(title=s,goal=s,tractable_question=s,rationale=s,knowns=arr(s),unknowns=arr(s),tasks=arr(ref('research_task')),source_ids=arr(s),limitations=arr(s))
schema=obj(assistant_message=s,action=enum('create_manifest','revise_manifest','explain','capability_gap','research_plan'),
           research_plan={'anyOf':[ref('research_plan'),{'type':'null'}]},
           manifest={'anyOf':[ref('manifest'),{'type':'null'}]},requested_capability=s,capability_gap_reason=s,suggested_extension=s,should_run=b)
schema['$defs']=d
path=ROOT/'backend/src/agent/proposal.schema.json'
path.write_text(json.dumps(schema,indent=2)+'\n',encoding='utf-8',newline='\n')
print(path)
