import ScientificModelViewer from './ScientificModelViewer';

/** Coordinate-aware rendering shared with Blender/scientific asset inspection. */
export default function MoleculeViewport({structure,selectedAtomIds=[],representation='surface',onAtomSelect}) {
  return <ScientificModelViewer structure={structure} selectedAtomIds={selectedAtomIds} representation={representation} style="microscopy"
    onInspect={inspection=>{if(inspection?.atom)onAtomSelect?.(Number.isFinite(Number(inspection.atom.id))?Number(inspection.atom.id):inspection.atom.id);}}/>
}
