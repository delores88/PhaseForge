import {
  Focus,
  MousePointer2,
  Rotate3D,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

const ELEMENT_COLORS = {
  H: 0xe8edf3,
  C: 0x667384,
  N: 0x4d7df3,
  O: 0xe55353,
  F: 0x73d673,
  P: 0xe6a23c,
  S: 0xe9cf4a,
  CL: 0x45c96b,
  BR: 0x9d4b37,
  I: 0x7357a6,
  FE: 0xc47a49,
  MG: 0x5bc66d,
  CA: 0x7a9bf5,
  ZN: 0x9da7b3,
};

const ELEMENT_RADII = {
  H: 0.24,
  C: 0.38,
  N: 0.36,
  O: 0.35,
  F: 0.34,
  P: 0.48,
  S: 0.46,
  CL: 0.45,
  BR: 0.49,
  I: 0.54,
  FE: 0.48,
  MG: 0.5,
  CA: 0.5,
  ZN: 0.46,
};

function elementKey(value) {
  return String(value || "C").trim().toUpperCase();
}

export default function MoleculeViewport({
  structure,
  selectedAtomIds = [],
  representation = "ball_and_stick",
  onAtomSelect,
}) {
  const hostRef = useRef(null);
  const [hoveredAtom, setHoveredAtom] = useState(null);
  const [rendererError, setRendererError] = useState(null);
  const selectedKey = useMemo(
    () => [...selectedAtomIds].map(Number).filter(Number.isFinite).sort((left, right) => left - right).join(","),
    [selectedAtomIds],
  );

  useEffect(() => {
    let cancelled = false;
    let cleanup = () => {};

    async function mount() {
      const host = hostRef.current;
      if (!host || !structure?.atoms?.length) return;
      try {
        const THREE = await import("three");
        if (cancelled) return;

        const renderer = new THREE.WebGLRenderer({
          antialias: true,
          alpha: false,
          powerPreference: "high-performance",
        });
        renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.6));
        renderer.setClearColor(0x0d131a, 1);
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        host.replaceChildren(renderer.domElement);

        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x0d131a);
        scene.fog = new THREE.FogExp2(0x0d131a, 0.012);
        const camera = new THREE.PerspectiveCamera(46, 1, 0.02, 100000);
        const root = new THREE.Group();
        scene.add(root);

        scene.add(new THREE.HemisphereLight(0xdcecff, 0x17202b, 1.8));
        const keyLight = new THREE.DirectionalLight(0xffffff, 2.6);
        keyLight.position.set(3, 5, 7);
        scene.add(keyLight);
        const rim = new THREE.DirectionalLight(0x76bff0, 1.2);
        rim.position.set(-5, -2, -4);
        scene.add(rim);

        const maxAtoms = 12000;
        const atoms = structure.atoms.slice(0, maxAtoms);
        const atomById = new Map(atoms.map((atom) => [Number(atom.id), atom]));
        const selected = new Set(selectedAtomIds.map(Number));
        const positions = atoms.map((atom) => new THREE.Vector3(...atom.position.map(Number)));
        const bounds = new THREE.Box3().setFromPoints(positions);
        const center = bounds.getCenter(new THREE.Vector3());
        const dimensions = bounds.getSize(new THREE.Vector3());
        const maxDimension = Math.max(dimensions.x, dimensions.y, dimensions.z, 2);

        const sphereGeometry = new THREE.SphereGeometry(
          1,
          atoms.length > 5000 ? 10 : 18,
          atoms.length > 5000 ? 8 : 13,
        );
        const material = new THREE.MeshStandardMaterial({
          roughness: 0.48,
          metalness: 0.04,
          vertexColors: true,
        });
        const atomMesh = new THREE.InstancedMesh(sphereGeometry, material, atoms.length);
        atomMesh.instanceMatrix.setUsage(THREE.StaticDrawUsage);
        atomMesh.userData.atomIds = atoms.map((atom) => Number(atom.id));
        const matrix = new THREE.Matrix4();
        const color = new THREE.Color();

        atoms.forEach((atom, index) => {
          const key = elementKey(atom.element);
          const baseRadius = ELEMENT_RADII[key] || 0.4;
          const radius = representation === "space_filling" ? baseRadius * 1.75 : baseRadius;
          const scale = selected.has(Number(atom.id)) ? radius * 1.35 : radius;
          matrix.compose(
            new THREE.Vector3(
              Number(atom.position[0]) - center.x,
              Number(atom.position[1]) - center.y,
              Number(atom.position[2]) - center.z,
            ),
            new THREE.Quaternion(),
            new THREE.Vector3(scale, scale, scale),
          );
          atomMesh.setMatrixAt(index, matrix);
          color.setHex(
            selected.has(Number(atom.id))
              ? 0xf2c66d
              : (ELEMENT_COLORS[key] || 0x9aa6b4),
          );
          atomMesh.setColorAt(index, color);
        });
        atomMesh.instanceMatrix.needsUpdate = true;
        if (atomMesh.instanceColor) atomMesh.instanceColor.needsUpdate = true;
        root.add(atomMesh);

        if (
          representation !== "space_filling" &&
          Array.isArray(structure.bonds) &&
          structure.bonds.length
        ) {
          const segmentValues = [];
          for (const bond of structure.bonds.slice(0, 20000)) {
            const left = atomById.get(Number(bond.atom_a));
            const right = atomById.get(Number(bond.atom_b));
            if (!left || !right) continue;
            segmentValues.push(
              Number(left.position[0]) - center.x,
              Number(left.position[1]) - center.y,
              Number(left.position[2]) - center.z,
              Number(right.position[0]) - center.x,
              Number(right.position[1]) - center.y,
              Number(right.position[2]) - center.z,
            );
          }
          const bondGeometry = new THREE.BufferGeometry();
          bondGeometry.setAttribute(
            "position",
            new THREE.Float32BufferAttribute(segmentValues, 3),
          );
          const bondMaterial = new THREE.LineBasicMaterial({
            color: 0x7b8797,
            transparent: true,
            opacity: 0.58,
          });
          root.add(new THREE.LineSegments(bondGeometry, bondMaterial));
        }

        const grid = new THREE.GridHelper(maxDimension * 2.6, 12, 0x304052, 0x1f2a36);
        grid.position.y = -dimensions.y / 2 - Math.max(0.8, maxDimension * 0.05);
        grid.material.transparent = true;
        grid.material.opacity = 0.35;
        scene.add(grid);

        let cameraDistance = maxDimension * 1.65 + 5;
        camera.position.set(cameraDistance * 0.72, cameraDistance * 0.42, cameraDistance);
        camera.lookAt(0, 0, 0);

        let drag = false;
        let moved = false;
        let priorX = 0;
        let priorY = 0;
        const raycaster = new THREE.Raycaster();
        const pointer = new THREE.Vector2();

        const resize = () => {
          const rect = host.getBoundingClientRect();
          const width = Math.max(1, rect.width);
          const height = Math.max(1, rect.height);
          renderer.setSize(width, height, false);
          camera.aspect = width / height;
          camera.updateProjectionMatrix();
        };
        resize();
        const observer = new ResizeObserver(resize);
        observer.observe(host);

        const pointerDown = (event) => {
          drag = true;
          moved = false;
          priorX = event.clientX;
          priorY = event.clientY;
          renderer.domElement.setPointerCapture?.(event.pointerId);
        };
        const pointerMove = (event) => {
          if (!drag) return;
          const dx = event.clientX - priorX;
          const dy = event.clientY - priorY;
          if (Math.abs(dx) + Math.abs(dy) > 2) moved = true;
          root.rotation.y += dx * 0.008;
          root.rotation.x = THREE.MathUtils.clamp(
            root.rotation.x + dy * 0.008,
            -Math.PI / 2,
            Math.PI / 2,
          );
          priorX = event.clientX;
          priorY = event.clientY;
        };
        const pointerUp = (event) => {
          drag = false;
          renderer.domElement.releasePointerCapture?.(event.pointerId);
          if (moved) return;
          const rect = renderer.domElement.getBoundingClientRect();
          pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
          pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
          raycaster.setFromCamera(pointer, camera);
          const hit = raycaster.intersectObject(atomMesh, false)[0];
          if (hit && Number.isInteger(hit.instanceId)) {
            const atomId = atomMesh.userData.atomIds[hit.instanceId];
            setHoveredAtom(atomById.get(atomId) || null);
            onAtomSelect?.(atomId);
          }
        };
        const wheel = (event) => {
          event.preventDefault();
          cameraDistance = THREE.MathUtils.clamp(
            cameraDistance * (1 + event.deltaY * 0.001),
            maxDimension * 0.45 + 1,
            maxDimension * 8 + 30,
          );
          camera.position.setLength(cameraDistance);
          camera.lookAt(0, 0, 0);
        };
        renderer.domElement.addEventListener("pointerdown", pointerDown);
        renderer.domElement.addEventListener("pointermove", pointerMove);
        renderer.domElement.addEventListener("pointerup", pointerUp);
        renderer.domElement.addEventListener("wheel", wheel, { passive: false });

        let animationFrame = 0;
        const draw = () => {
          animationFrame = window.requestAnimationFrame(draw);
          renderer.render(scene, camera);
        };
        draw();

        cleanup = () => {
          window.cancelAnimationFrame(animationFrame);
          observer.disconnect();
          renderer.domElement.removeEventListener("pointerdown", pointerDown);
          renderer.domElement.removeEventListener("pointermove", pointerMove);
          renderer.domElement.removeEventListener("pointerup", pointerUp);
          renderer.domElement.removeEventListener("wheel", wheel);
          scene.traverse((object) => {
            object.geometry?.dispose?.();
            if (Array.isArray(object.material)) {
              object.material.forEach((entry) => entry.dispose?.());
            } else {
              object.material?.dispose?.();
            }
          });
          renderer.dispose();
          host.replaceChildren();
        };
      } catch (error) {
        setRendererError(error.message || "The molecular renderer could not initialize.");
      }
    }

    setRendererError(null);
    mount();
    return () => {
      cancelled = true;
      cleanup();
    };
  }, [structure, selectedKey, representation, onAtomSelect]);

  if (!structure?.atoms?.length) {
    return (
      <div className="moleculeViewport moleculeViewport--empty">
        <Focus size={28} />
        <strong>No molecular structure selected</strong>
        <span>
          Import PDB, SDF, MOL, or XYZ coordinates to create a shared human/agent world state.
        </span>
      </div>
    );
  }

  return (
    <div className="moleculeViewport">
      <div ref={hostRef} className="moleculeViewport__canvas" />
      <div className="moleculeViewport__hint">
        <Rotate3D size={13} /> Drag to rotate · wheel to zoom · click an atom
      </div>
      <div className="moleculeViewport__count">
        {structure.atoms.length.toLocaleString()} atoms · {structure.bonds?.length?.toLocaleString?.() || 0} bonds
      </div>
      {structure.atoms.length > 12000 && (
        <div className="moleculeViewport__limit">
          Viewport sampled to 12,000 atoms. Diagnostics use the complete imported record.
        </div>
      )}
      {hoveredAtom && (
        <div className="moleculeAtomCard">
          <MousePointer2 size={13} />
          <div>
            <strong>{hoveredAtom.element} · {hoveredAtom.name}</strong>
            <span>
              {hoveredAtom.residue_name || "No residue"} {hoveredAtom.residue_id || ""}
              {hoveredAtom.chain_id ? ` · chain ${hoveredAtom.chain_id}` : ""}
            </span>
            <code>{hoveredAtom.position.map((value) => Number(value).toFixed(3)).join(", ")}</code>
          </div>
          <button type="button" onClick={() => setHoveredAtom(null)} aria-label="Close atom details">×</button>
        </div>
      )}
      {rendererError && <div className="moleculeViewport__error">{rendererError}</div>}
    </div>
  );
}
