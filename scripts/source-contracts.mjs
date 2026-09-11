import vm from 'node:vm';

// Exercise the actual credential button predicate across busy/key/model states.
// JSX rendering, network calls, and provider credential storage are separate tests.
export function credentialSaveContract(source){
  const buttons=[...source.matchAll(/<button\b[\s\S]*?<\/button>/g)].map(match=>match[0]).filter(button=>/onClick=\{[^}]*saveKey\(/.test(button));
  if(buttons.length!==1)return false;
  const expression=buttons[0].match(/\bdisabled=\{([^{}]+)\}/)?.[1];
  if(!expression)return false;
  for(const working of [false,true])for(const key of ['', '   ', 'fixture-key'])for(const model of ['', 'fixture-model']){
    const context={working,form:{apiKey:key,api_key:key,model},model,providerReady:!!model,selectedModel:model,selection:{model}};
    try{
      const result=vm.runInNewContext(`(${expression})`,context,{timeout:25});
      if(result!==(working||!key.trim()))return false;
    }catch{return false;}
  }
  return true;
}

export function cannedPhysicsMarkers(source){
  const disclaimerPositions=new Set();
  // Only an explicitly unsupported closed-form solution in a capability scope
  // is exempt. Named preset keys, functions, constants and other matches remain.
  for(const scope of source.matchAll(/"scope"\s*:\s*"(?:[^"\\]|\\.)*"/g)){
    for(const sentence of scope[0].matchAll(/\bNo\b[^.!?"\\]*?\b(?:general\s+)?closed-form\s+three-body\s+solution\b/gi)){
      disclaimerPositions.add(scope.index+sentence.index+sentence[0].toLowerCase().lastIndexOf('three-body'));
    }
  }
  return [...source.matchAll(/three[-_ ]body|figure[-_ ]eight|electron[-_ ]collapse|known[-_ ]answer[-_ ]seed/gi)]
    .filter(match=>!disclaimerPositions.has(match.index)).map(match=>({index:match.index,marker:match[0]}));
}
