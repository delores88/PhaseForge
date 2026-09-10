#!/usr/bin/env python3
"""Independent, standard-library ODE replay for PhaseForge exported research bundles.

No eval, shell, imports from model input, network, GPU or PhaseForge runtime required.
Uses a separately implemented Euler/RK4 solver and a restricted arithmetic AST.
It tests a numerical implementation, not the scientific model or novelty of a result.
"""
from __future__ import annotations
import argparse
import ast
import json
import math
import operator
import pathlib
import time

UNARY={ast.UAdd:operator.pos,ast.USub:operator.neg}
BINARY={ast.Add:operator.add,ast.Sub:operator.sub,ast.Mult:operator.mul,ast.Div:operator.truediv,ast.Pow:math.pow}
FUNCTIONS={name:getattr(math,name) for name in ('sqrt','exp','sin','cos','tan','asin','acos','atan','atan2','sinh','cosh','tanh','floor','ceil')}
FUNCTIONS.update(abs=abs,ln=math.log,log=math.log,min=min,max=max,pow=math.pow,
                 clamp=lambda x,lo,hi:min(hi,max(lo,x)),
                 sign=lambda x:math.copysign(1.0,x),
                 round=lambda x:math.copysign(math.floor(abs(x)+0.5),x))
ARITY={name:1 for name in FUNCTIONS}
ARITY.update(atan2=2,min=2,max=2,pow=2,clamp=3)


def expression(source: str):
    if not isinstance(source,str) or not source.strip() or len(source)>8192:
        raise ValueError('Invalid bounded expression')
    tree=ast.parse(source.replace('^','**'),mode='eval')
    if sum(1 for _ in ast.walk(tree))>2048:raise ValueError('Expression too complex')
    def compile_node(node,depth=0):
        if depth>64:raise ValueError('Expression nesting limit exceeded')
        if isinstance(node,ast.Expression):return compile_node(node.body,depth+1)
        if isinstance(node,ast.Constant) and type(node.value) in (int,float):
            value=float(node.value)
            if not math.isfinite(value):raise ValueError('Nonfinite literal')
            return lambda env:value
        if isinstance(node,ast.Name):
            name=node.id
            if name in ('pi','e'):return lambda env: getattr(math,name)
            return lambda env:env[name]
        if isinstance(node,ast.BinOp) and type(node.op) in BINARY:
            a,b=compile_node(node.left,depth+1),compile_node(node.right,depth+1);fn=BINARY[type(node.op)]
            return lambda env:fn(a(env),b(env))
        if isinstance(node,ast.UnaryOp) and type(node.op) in UNARY:
            a=compile_node(node.operand,depth+1);fn=UNARY[type(node.op)]
            return lambda env:fn(a(env))
        if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id in FUNCTIONS and not node.keywords:
            name=node.func.id
            if len(node.args)!=ARITY[name]:raise ValueError(f'Wrong arity: {name}')
            parts=[compile_node(n,depth+1) for n in node.args];fn=FUNCTIONS[name]
            return lambda env:fn(*(part(env) for part in parts))
        raise ValueError(f'Forbidden expression node: {type(node).__name__}')
    return compile_node(tree)


class TrajectoryTracker:
    """Independent sampled reductions; no invented sub-step events or extra ODE states."""
    OPERATIONS={'minimum','maximum','range','time_mean','integral','duration_below','duration_above','entries_below','entries_above','first_below','first_above'}
    def __init__(self,specs,context,time):
        if len(specs)>64:raise ValueError('At most 64 trajectory measures')
        self.start=self.time=time;self.rows=[];seen=set()
        for spec in specs:
            kind=spec['reducer'];name=spec['name'];threshold=float(spec.get('threshold',0));band=float(spec.get('hysteresis',0))
            if kind not in self.OPERATIONS or name in seen or not math.isfinite(threshold) or not math.isfinite(band) or band<0:raise ValueError('Invalid trajectory measure')
            seen.add(name);fn=expression(spec['expression']);v=float(fn(context))
            if not math.isfinite(v):raise ValueError('Nonfinite initial measure')
            inside=v>threshold if kind in ('entries_above','first_above') else v<threshold
            self.rows.append(dict(spec=spec,fn=fn,last=v,low=v,high=v,area=0.,below=0.,above=0.,corrections=[0.,0.,0.],threshold=threshold,band=band,inside=inside,entries=0,first=0. if inside else None))
    @staticmethod
    def fraction(a,b,k):
        if a<k and b<k:return 1.
        if a>=k and b>=k:return 0.
        f=max(0.,min(1.,(k-a)/(b-a)))
        return f if a<k else 1.-f
    @staticmethod
    def add(row,key,value,index):
        y=value-row['corrections'][index];n=row[key]+y
        row['corrections'][index]=(n-row[key])-y;row[key]=n
    def observe(self,context,time):
        dt=time-self.time
        if not math.isfinite(dt) or dt<=0:raise ValueError('Nonmonotonic sample time')
        for r in self.rows:
            v=float(r['fn'](context));k=r['threshold'];h=r['band']
            if not math.isfinite(v):raise ValueError('Nonfinite trajectory measure')
            r['low']=min(r['low'],v);r['high']=max(r['high'],v)
            self.add(r,'area',dt*(.5*r['last']+.5*v),0)
            self.add(r,'below',dt*self.fraction(r['last'],v,k),1)
            self.add(r,'above',dt*self.fraction(-r['last'],-v,-k),2)
            x,k=(-v,-k) if r['spec']['reducer'] in ('entries_above','first_above') else (v,k)
            if not r['inside'] and x<k-h:
                r['inside']=True;r['entries']+=1
                if r['first'] is None:r['first']=time-self.start
            elif r['inside'] and x>=k+h:r['inside']=False
            if not all(math.isfinite(r[x]) for x in ('area','below','above')):raise ValueError('Trajectory accumulation overflow')
            r['last']=v
        self.time=time
    def values(self):
        out={};elapsed=self.time-self.start
        for r in self.rows:
            k=r['spec']['reducer'];name=r['spec']['name']
            value={'minimum':r['low'],'maximum':r['high'],'range':r['high']-r['low'],'integral':r['area'],'time_mean':r['area']/elapsed if elapsed else r['last'],
                'duration_below':r['below'],'duration_above':r['above'],'entries_below':r['entries'],'entries_above':r['entries'],
                'first_below':r['first'] if r['first'] is not None else elapsed,'first_above':r['first'] if r['first'] is not None else elapsed}[k]
            if not math.isfinite(value):raise ValueError('Nonfinite reduced measurement')
            out[name]=value
            if k in ('first_below','first_above'):out[name+'_observed']=float(r['first'] is not None)
        return out


def replay(manifest:dict,run:dict|None=None,refinement:int=1,max_steps:int=1000000,max_seconds:float=60,*,consume_rhs=None)->dict:
    if manifest['model']['kind']!='state_vector_ode':raise ValueError('Independent verifier supports state_vector_ode only; pairwise workers need a separate implementation')
    if refinement not in (1,2,4,8):raise ValueError('Refinement must be 1, 2, 4 or 8')
    spec=manifest['integration'];method=spec['method']
    if method not in ('euler','rk4'):raise ValueError('Unsupported reference integration method')
    variables=manifest['model']['variables'];names=[v['name'] for v in variables]
    if len(set(names))!=len(names) or not 1<=len(names)<=128:raise ValueError('Invalid variable identifiers/dimension')
    initial=[float(v['initial']) for v in variables];constants={c['name']:float(c['value']) for c in manifest.get('constants',[])}
    best=(run or {}).get('result',{}).get('best_candidate',{})
    for v in manifest.get('search',{}).get('variables',[]):
        if v['name'] not in best:continue
        target=v['target'];value=float(best[v['name']])
        if target.startswith('initial:'):initial[names.index(target[8:])]=value
        elif target.startswith('constant:'):constants[target[9:]]=value
        else:raise ValueError('Unsupported winning-candidate target')
    derivatives={d['variable']:expression(d['expression']) for d in manifest['model']['derivatives']}
    if set(derivatives)!=set(names):raise ValueError('One derivative per state required')
    start=float(spec['start_time']);end=float(spec['end_time']);dt=float(spec['time_step'])/refinement
    if not all(math.isfinite(v) for v in [*initial,*constants.values(),start,end,dt]) or dt<=0 or end<=start:raise ValueError('Nonfinite or invalid integration inputs')
    needed=math.ceil((end-start)/dt);limit=int(spec['max_steps'])*refinement
    if needed>max_steps or needed>limit:raise ValueError(f'Complete endpoint requires {needed} steps, above allowed limit')
    y=list(initial);lo=list(y);hi=list(y);sums=list(y);t=start;count=0;deadline=time.monotonic()+max_seconds
    def measurement_context(state,at):return {**constants,**dict(zip(names,state)),**{f'initial_{name}':initial[i] for i,name in enumerate(names)},'t':at}
    tracker=TrajectoryTracker(manifest.get('trajectory',[]),measurement_context(y,t),t)
    def rhs(timepoint,state):
        if consume_rhs is not None:consume_rhs()
        env={**constants,**dict(zip(names,state)),'t':timepoint}
        return [float(derivatives[name](env)) for name in names]
    for _ in range(needed):
        if count%128==0 and time.monotonic()>deadline:raise TimeoutError('Independent replay time budget exceeded')
        h=min(dt,end-t)
        if h<=0:break
        k1=rhs(t,y)
        if method=='euler':y=[a+h*b for a,b in zip(y,k1)]
        else:
            k2=rhs(t+h/2,[a+h*b/2 for a,b in zip(y,k1)])
            k3=rhs(t+h/2,[a+h*b/2 for a,b in zip(y,k2)])
            k4=rhs(t+h,[a+h*b for a,b in zip(y,k3)])
            y=[a+h*(b+2*c+2*d+e)/6 for a,b,c,d,e in zip(y,k1,k2,k3,k4)]
        if not all(math.isfinite(v) for v in y):raise ValueError('Reference state became nonfinite')
        t+=h;count+=1
        if tracker.rows:tracker.observe(measurement_context(y,t),t)
        for i,value in enumerate(y):lo[i]=min(lo[i],value);hi[i]=max(hi[i],value);sums[i]+=value
    context={**constants,**dict(zip(names,y)),'t':t,'elapsed_time':t-start,'steps':count,'sample_count':count+1}
    for i,name in enumerate(names):
        context.update({f'initial_{name}':initial[i],f'final_{name}':y[i],f'minimum_{name}':lo[i],f'maximum_{name}':hi[i],
                        f'mean_{name}':sums[i]/(count+1),f'delta_{name}':y[i]-initial[i],f'abs_delta_{name}':abs(y[i]-initial[i])})
    for v in manifest.get('search',{}).get('variables',[]):
        target=v['target']
        if target.startswith('initial:'):context[v['name']]=initial[names.index(target[8:])]
        elif target.startswith('constant:'):context[v['name']]=constants[target[9:]]
    context.update(tracker.values())
    metrics={key:value for key,value in context.items() if key.startswith(('final_','delta_','abs_delta_')) or key in ('elapsed_time','steps','sample_count')}
    metrics.update(tracker.values())
    for obs in manifest.get('observables',[]):metrics[obs['name']]=float(expression(obs['expression'])(context))
    if not all(math.isfinite(v) for v in metrics.values()):raise ValueError('Nonfinite reference metrics')
    constraints=[{'name':c['name'],'value':expression(c['expression'])(context),'tolerance':c['tolerance']} for c in manifest.get('constraints',[])]
    for c in constraints:c['passed']=math.isfinite(c['value']) and c['value']<=c['tolerance']
    return {'engine':'independent-python-stdlib-reference/1','method':method,'refinement':refinement,'dt':dt,'steps':count,'actual_end_time':t,
            'reached_end_time':abs(t-end)<=1e-9*max(1,abs(end)),'metrics':metrics,'constraints':constraints,
            'boundary':'Independent implementation check only; not an independent physical theory, formal proof, or scientific novelty check.'}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('manifest',type=pathlib.Path);p.add_argument('--run',type=pathlib.Path)
    p.add_argument('--refinement',type=int,default=1);p.add_argument('--max-steps',type=int,default=1000000)
    p.add_argument('--max-seconds',type=float,default=60);p.add_argument('--absolute-tolerance',type=float,default=1e-8)
    p.add_argument('--relative-tolerance',type=float,default=1e-6);p.add_argument('--out',type=pathlib.Path)
    args=p.parse_args()
    if not all(math.isfinite(v) and v>=0 for v in (args.absolute_tolerance,args.relative_tolerance)) or not 0<args.max_seconds<=3600:
        p.error('Finite nonnegative tolerances and max-seconds in (0,3600] required')
    run=json.loads(args.run.read_text(encoding='utf-8-sig')) if args.run else None
    manifest=json.loads(args.manifest.read_text(encoding='utf-8-sig'))
    if run and run.get('manifest_id')!=manifest.get('id'):p.error('Run and manifest provenance mismatch')
    out=replay(manifest,run,args.refinement,args.max_steps,args.max_seconds)
    if run:
        old=run.get('result',{}).get('metrics',{});checks=[]
        for name,value in out['metrics'].items():
            if not name.startswith('final_') and name not in {o['name'] for o in manifest.get('observables',[])}:continue
            baseline=old.get(name)
            valid=type(baseline) in (float,int) and math.isfinite(baseline)
            tolerance=args.absolute_tolerance+args.relative_tolerance*max(abs(value),abs(baseline) if valid else 0)
            checks.append({'metric':name,'reference':value,'recorded':baseline,'threshold':tolerance,'agreement':valid and abs(value-baseline)<=tolerance})
        out['comparisons']=checks;out['status']='agreement' if checks and all(c['agreement'] for c in checks) and out['reached_end_time'] else 'inconclusive_or_disagrees'
    text=json.dumps(out,indent=2,allow_nan=False)
    if args.out:args.out.write_text(text+'\n',encoding='utf-8')
    print(text)
    if run and out['status']!='agreement':raise SystemExit(2)

if __name__=='__main__':main()
