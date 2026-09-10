#!/usr/bin/env python3
"""PhaseForge bounded independent verification worker (Python standard library).

Only executable code shipped by PhaseForge is loaded. Inputs are arithmetic
expressions interpreted by the separate reference AST, never Python code.
Output is evidence about a model and a frozen protocol, NOT a novelty verdict.
"""
from __future__ import annotations
import copy
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import random
import sys
import time

# Explicitly load the trusted companion, also when invoked with python -I.
_here = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('phaseforge_reference', _here / 'reference_ode.py')
ref = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ref)

ENGINE = 'phaseforge-independent-dp54/1'
C = (0., 1/5, 3/10, 4/5, 8/9, 1., 1.)
A = ((), (1/5,), (3/40, 9/40), (44/45, -56/15, 32/9),
     (19372/6561, -25360/2187, 64448/6561, -212/729),
     (9017/3168, -355/33, 46732/5247, 49/176, -5103/18656),
     (35/384, 0., 500/1113, 125/192, -2187/6784, 11/84))
B5 = (35/384, 0., 500/1113, 125/192, -2187/6784, 11/84, 0.)
B4 = (5179/57600, 0., 7571/16695, 393/640, -92097/339200, 187/2100, 1/40)


def digest_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def finite(v):
    return type(v) in (int, float) and math.isfinite(v)


class Budget:
    def __init__(self, evaluations, seconds):
        if type(evaluations) is not int or not 100 <= evaluations <= 20_000_000:
            raise ValueError('RHS budget must be 100-20,000,000')
        if not finite(seconds) or not 1 <= seconds <= 3600:
            raise ValueError('Wall budget must be 1-3,600 seconds')
        self.limit, self.used = evaluations, 0
        self.started = time.monotonic()
        self.deadline = self.started + seconds

    def charge(self, amount=1):
        if self.used + amount > self.limit:
            raise TimeoutError('Approved total RHS evaluation budget exhausted')
        if time.monotonic() >= self.deadline:
            raise TimeoutError('Approved verification wall-time budget exhausted')
        self.used += amount

    def remaining_seconds(self):
        self.charge(0)
        return max(.001, self.deadline - time.monotonic())


def adaptive_replay(manifest, budget, atol, rtol):
    """Dormand-Prince 5(4), independently coded. Metrics sampled on original h grid.

    Internal adaptive steps are distinct from observation intervals: sums, minima,
    maxima and sample_count use the original integration grid, preserving the
    meaning of native summary observables rather than silently changing them.
    """
    if not finite(atol) or not finite(rtol) or atol <= 0 or rtol <= 0:
        raise ValueError('Positive finite solver tolerances required')
    model = manifest['model']
    if model['kind'] != 'state_vector_ode':
        raise ValueError('Independent execution currently supports ODEs only')
    variables = model['variables']
    names = [v['name'] for v in variables]
    if not 1 <= len(names) <= 128 or len(set(names)) != len(names):
        raise ValueError('Invalid ODE dimension or duplicate state name')
    initial = [float(v['initial']) for v in variables]
    constants = {v['name']:float(v['value']) for v in manifest.get('constants', [])}
    deriv = {d['variable']: ref.expression(d['expression']) for d in model['derivatives']}
    if set(deriv) != set(names):
        raise ValueError('One derivative per state required')
    spec = manifest['integration']
    start, end, grid = map(float, (spec['start_time'], spec['end_time'], spec['time_step']))
    if not all(finite(x) for x in [start,end,grid,*initial,*constants.values()]) or grid <= 0 or end <= start:
        raise ValueError('Invalid finite integration domain')
    needed = math.ceil((end-start)/grid)
    if needed > min(int(spec['max_steps']), 2_000_000):
        raise ValueError('Complete observation grid exceeds approved limits')
    y = list(initial); lo=list(y); hi=list(y); sums=list(y)
    t=start; sample_t=start; samples=0; accepted=0; rejected=0; h=grid
    def measurement_context(state,at):return {**constants,**dict(zip(names,state)),**{f'initial_{name}':initial[i] for i,name in enumerate(names)},'t':at}
    tracker=ref.TrajectoryTracker(manifest.get('trajectory',[]),measurement_context(y,t),t)

    def rhs(timepoint, state):
        budget.charge()
        env={**constants,**dict(zip(names,state)),'t':timepoint}
        out=[float(deriv[name](env)) for name in names]
        if not all(map(finite,out)):
            raise ValueError('Nonfinite derivative')
        return out

    for _ in range(needed):
        # Same recursive time stepping as the baseline to avoid changed sample semantics.
        target = sample_t + min(grid, end-sample_t)
        if target <= sample_t:break
        while t < target:
            h=min(h,target-t)
            if t+h == t:
                raise ValueError('Adaptive step underflow; tolerance cannot be met')
            stages=[]
            for i in range(7):
                yi=[v+h*sum(A[i][j]*stages[j][k] for j in range(i)) for k,v in enumerate(y)]
                stages.append(rhs(t+C[i]*h,yi))
            y5=[v+h*sum(B5[j]*stages[j][k] for j in range(7)) for k,v in enumerate(y)]
            y4=[v+h*sum(B4[j]*stages[j][k] for j in range(7)) for k,v in enumerate(y)]
            if not all(map(finite,y5+y4)):
                raise ValueError('Nonfinite adaptive state')
            err=max(abs(a-b)/(atol+rtol*max(abs(v),abs(a))) for a,b,v in zip(y5,y4,y))
            factor=5.0 if err == 0 else min(5.0,max(.1,.9*err**(-.2)))
            if err <= 1:
                y=y5;t+=h;accepted+=1
                if abs(target-t)<=2*math.ulp(max(abs(target),1.0)):t=target
            else:rejected+=1
            h*=factor
        sample_t=target;samples+=1
        if tracker.rows:tracker.observe(measurement_context(y,sample_t),sample_t)
        for i,v in enumerate(y):
            lo[i]=min(lo[i],v);hi[i]=max(hi[i],v);sums[i]+=v
    context={**constants,**dict(zip(names,y)),'t':sample_t,'elapsed_time':sample_t-start,'steps':samples,'sample_count':samples+1}
    for i,name in enumerate(names):
        context.update({f'initial_{name}':initial[i],f'final_{name}':y[i],f'minimum_{name}':lo[i],f'maximum_{name}':hi[i],
                        f'mean_{name}':sums[i]/(samples+1),f'delta_{name}':y[i]-initial[i],f'abs_delta_{name}':abs(y[i]-initial[i])})
    for alias in manifest.get('search',{}).get('variables',[]):
        target=alias['target']
        if target.startswith('initial:'):context[alias['name']]=initial[names.index(target[8:])]
        elif target.startswith('constant:'):context[alias['name']]=constants[target[9:]]
    context.update(tracker.values())
    metrics={k:v for k,v in context.items() if k.startswith(('final_','delta_','abs_delta_')) or k in ('elapsed_time','steps','sample_count')}
    metrics.update(tracker.values())
    for obs in manifest.get('observables',[]):
        metrics[obs['name']]=float(ref.expression(obs['expression'])(context))
    if not all(map(finite,metrics.values())):raise ValueError('Nonfinite observable')
    constraints=[]
    for c in manifest.get('constraints',[]):
        value=float(ref.expression(c['expression'])(context))
        constraints.append({'name':c['name'],'value':value,'tolerance':c['tolerance'],'passed':finite(value) and value<=c['tolerance']})
    return {'engine':ENGINE,'method':'dormand_prince_5_4','observation_grid':grid,'actual_end_time':sample_t,
            'reached_end_time':abs(sample_t-end)<=1e-9*max(1,abs(end)), 'steps':samples,'accepted_internal_steps':accepted,
            'rejected_internal_steps':rejected,'metrics':metrics,'constraints':constraints,'solver_atol':atol,'solver_rtol':rtol}


def checks(recorded, observed, rules, robustness=False):
    rows=[]
    for rule in rules:
        name=rule['metric'];a=recorded.get(name);b=observed.get(name)
        valid=finite(a) and finite(b)
        limit=rule['robustness_absolute'] if robustness else rule['absolute_tolerance']+rule['relative_tolerance']*max(abs(a or 0),abs(b or 0))
        delta=abs(a-b) if valid else None
        if not finite(limit) or (delta is not None and not finite(delta)):valid=False
        rows.append({'metric':name,'recorded':a,'independent':b,'absolute_difference':delta if valid else None,
                     'threshold':limit if finite(limit) else None,'status':'agreement' if valid and delta<=limit else 'disagreement' if valid else 'inconclusive'})
    return rows


def set_target(manifest,target,value):
    if not finite(value):raise ValueError('Nonfinite parameter value')
    if target.startswith('initial:'):
        entries=manifest['model']['variables'];key='initial';name=target[8:]
    elif target.startswith('constant:'):
        entries=manifest.get('constants',[]);key='value';name=target[9:]
    else:raise ValueError('Holdout worker supports initial: and constant: targets only')
    entry=next((v for v in entries if v['name']==name),None)
    if entry is None:raise ValueError('Unknown holdout target')
    entry[key]=value


def get_target(manifest,target):
    if target.startswith('initial:'):return next(v['initial'] for v in manifest['model']['variables'] if v['name']==target[8:])
    if target.startswith('constant:'):return next(v['value'] for v in manifest['constants'] if v['name']==target[9:])
    raise ValueError('Unsupported holdout target')


def endpoint_ok(result):
    return result.get('reached_end_time') is True and all(c.get('passed') is True for c in result.get('constraints',[]))


def evaluate(payload, progress=lambda *_:None, checkpoint=lambda *_:None):
    p=payload['protocol'];manifest=payload['manifest'];run=payload['source_run']
    if run['manifest_id']!=manifest['id'] or run['project_id']!=manifest['project_id'] or run['status']!='completed':
        raise ValueError('Frozen run/manifest provenance mismatch')
    if run.get('result',{}).get('numerical',{}).get('reached_end_time') is not True:
        raise ValueError('Source evidence did not reach endpoint')
    if manifest.get('search',{}).get('enabled'):
        raise ValueError('Freeze a single candidate manifest; nested searches are not independent replay input')
    if not 1<=len(p['metrics'])<=8 or len({r['metric'] for r in p['metrics']})!=len(p['metrics']):raise ValueError('Invalid metric rules')
    for r in p['metrics']:
        if not all(finite(r[k]) and r[k]>=0 for k in ('absolute_tolerance','relative_tolerance','robustness_absolute')):
            raise ValueError('Invalid tolerances')
    replicas=p['holdout_replicates']
    if type(replicas) is not int or not 2<=replicas<=16:raise ValueError('Use 2-16 holdout perturbations')
    if not finite(p['perturb_fraction']) or not 0<p['perturb_fraction']<=.25:raise ValueError('Perturbation fraction outside (0,.25]')
    parameters=payload['parameters']
    if not 1<=len(parameters)<=12:raise ValueError('Holdout requires 1-12 frozen numerical parameters')
    for param in parameters:
        lo,hi=param['minimum'],param['maximum']
        if not finite(lo) or not finite(hi) or hi<=lo or not finite(hi-lo):raise ValueError('Invalid holdout bounds')
        nominal=get_target(manifest,param['target'])
        if not finite(nominal) or not lo<=nominal<=hi:raise ValueError('Frozen candidate parameter is outside the declared study bounds')
    budget=Budget(p['max_rhs_evaluations'],p['wall_seconds'])
    total=2+len(payload.get('controls',[]))+replicas
    report={'engine':ENGINE,'python_version':sys.version,'protocol_hash':payload['protocol_hash'],
            'state':'running','same_method':None,'different_method':None,'controls':[],'holdouts':[],
            'warning':'New perturbations characterize robustness after selection, not independent population samples or a multiple-testing-corrected significance test.'}
    def save_partial():
        report['rhs_evaluations']=budget.used
        report['wall_seconds']=time.monotonic()-budget.started
        checkpoint(report)
    try:
        progress('Independent Euler/RK4 implementation',0,total)
        same=ref.replay(manifest,run,max_steps=2_000_000,max_seconds=budget.remaining_seconds(),consume_rhs=budget.charge)
        same['comparisons']=checks(run['result']['metrics'],same['metrics'],p['metrics'])
        same['status']='agreement' if endpoint_ok(same) and all(c['status']=='agreement' for c in same['comparisons']) else 'disagreement_or_incomplete'
        report['same_method']=same
        save_partial()
        progress('Independent adaptive Dormand–Prince solver',1,total)
        adaptive=adaptive_replay(manifest,budget,p['solver_absolute_tolerance'],p['solver_relative_tolerance'])
        adaptive['comparisons']=checks(run['result']['metrics'],adaptive['metrics'],p['metrics'])
        adaptive['status']='agreement' if endpoint_ok(adaptive) and all(c['status']=='agreement' for c in adaptive['comparisons']) else 'disagreement_or_incomplete'
        report['different_method']=adaptive
        save_partial()
        for i,control in enumerate(payload.get('controls',[])):
            progress('Checking declared calibration controls',2+i,total)
            spec=control['spec'];cm=control['manifest'];cr=control['run']
            if cr['id']!=spec['run_id'] or cr['manifest_id']!=cm['id'] or cr['status']!='completed':raise ValueError('Control provenance mismatch')
            result=adaptive_replay(cm,budget,p['solver_absolute_tolerance'],p['solver_relative_tolerance'])
            val=result['metrics'].get(spec['metric']);lo=spec['minimum'];hi=spec['maximum']
            passed=finite(val) and finite(lo) and finite(hi) and lo<=val<=hi and endpoint_ok(result)
            report['controls'].append({'spec':spec,'observed':val,'status':'passed' if passed else 'failed','result':result})
            save_partial()
        rng=random.Random(p['holdout_seed']);seen=set();base=adaptive['metrics']
        for index in range(replicas):
            progress('Evaluating new bounded perturbations',2+len(report['controls'])+index,total)
            altered=copy.deepcopy(manifest);values=[]
            for param in parameters:
                lo,hi=param['minimum'],param['maximum'];nominal=float(get_target(manifest,param['target']))
                # Reflect at bounds rather than clipping multiple draws to an identical endpoint.
                width=hi-lo;raw=(nominal-lo)/width+(rng.random()*2-1)*p['perturb_fraction']
                folded=raw%2.;value=lo+width*(folded if folded<=1. else 2.-folded)
                set_target(altered,param['target'],value);values.append(value)
            distinct=tuple(values) not in seen;seen.add(tuple(values))
            result=adaptive_replay(altered,budget,p['solver_absolute_tolerance'],p['solver_relative_tolerance'])
            compared=checks(base,result['metrics'],p['metrics'],robustness=True)
            changed=any(v!=float(get_target(manifest,param['target'])) for param,v in zip(parameters,values))
            passed=distinct and changed and endpoint_ok(result) and all(c['status']=='agreement' for c in compared)
            report['holdouts'].append({'index':index+1,'values':values,'changed':changed,'unique':distinct,'comparisons':compared,
                                       'status':'within_declared_limits' if passed else 'changed_or_invalid','result':result})
            save_partial()
        report['state']='completed'
        report['independent_agreement']=same['status']=='agreement' and adaptive['status']=='agreement'
        report['controls_passed']=bool(report['controls']) and all(c['status']=='passed' for c in report['controls'])
        report['robustness']='within_declared_limits' if all(h['status']=='within_declared_limits' for h in report['holdouts']) else 'sensitive_or_invalid'
        progress('Verification evidence retained',total,total)
    except Exception as exc:
        report['state']='budget_exhausted' if isinstance(exc,TimeoutError) else 'failed'
        report['error']=str(exc)[:2000]
    report['rhs_evaluations']=budget.used
    report['wall_seconds']=time.monotonic()-budget.started
    return report


def atomic_json(path,obj):
    temp=path.with_suffix('.partial')
    temp.write_text(json.dumps(obj,separators=(',',':'),allow_nan=False),encoding='utf-8')
    os.replace(temp,path)


def main():
    if len(sys.argv)!=2:raise SystemExit('usage: verification_worker.py JOB_DIRECTORY')
    folder=Path(sys.argv[1]).resolve()
    raw=(folder/'input.json').read_bytes()
    if len(raw)>64*1024*1024:raise SystemExit('Input exceeds 64 MiB boundary')
    payload=json.loads(raw)
    if digest_file(__file__)!=payload['worker_source_hash'] or digest_file(_here/'reference_ode.py')!=payload['reference_source_hash']:
        raise SystemExit('Trusted worker source hashes changed')
    def progress(stage,completed,total):atomic_json(folder/'progress.json',{'stage':stage,'completed':completed,'total':total})
    envelope={'input_sha256':hashlib.sha256(raw).hexdigest(),'worker_source_hash':digest_file(__file__),'reference_source_hash':digest_file(_here/'reference_ode.py')}
    def checkpoint(report):atomic_json(folder/'checkpoint.json',{**report,**envelope,'partial':True})
    result=evaluate(payload,progress,checkpoint)
    result.update(**envelope)
    atomic_json(folder/'output.json',result)

if __name__=='__main__':main()
