const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const crypto = require('node:crypto');

// Exercise the real builder's shared-header -> generated-license -> template
// ordering, including its separate uninstaller compilation. No scientific
// runtime, real application payload, or model credentials are involved.
test('actual electron-builder installer and uninstaller preserve agreement ordering',
  {skip: process.platform !== 'win32', timeout: 180000}, async () => {
    const root = path.resolve(__dirname, '../..');
    const output = process.env.PHASEFORGE_INSTALLER_TEST_OUTPUT
      ? path.resolve(process.env.PHASEFORGE_INSTALLER_TEST_OUTPUT)
      : await fs.mkdtemp(path.join(os.tmpdir(), 'phaseforge-agreement-'));
    await fs.mkdir(output, {recursive:true});
    const project = path.join(output, 'project/desktop');
    const payload = path.join(project, 'dist/win-unpacked');
    await fs.mkdir(path.join(project, 'build'), {recursive:true});
    await fs.mkdir(payload, {recursive:true});
    await fs.mkdir(path.join(payload, 'resources'), {recursive:true});
    const terms = '../tools/third-party/msvc-runtime/END-USER-TERMS.txt';
    await fs.mkdir(path.dirname(path.resolve(project, terms)), {recursive:true});
    await fs.copyFile(path.join(root, 'tools/third-party/msvc-runtime/END-USER-TERMS.txt'), path.resolve(project, terms));
    await fs.copyFile(path.join(root, 'desktop/build/installer.nsh'), path.join(project, 'build/installer.nsh'));
    const name = 'PhaseForge Agreement Fixture';
    // A deliberately non-executable file is enough to test NSIS packaging;
    // runAfterFinish is disabled and this payload is never launched.
    await fs.writeFile(path.join(payload, `${name}.exe`), 'Installer agreement contract fixture; not an application executable.\n');
    await fs.writeFile(path.join(project, 'package.json'), JSON.stringify({
      name:'phaseforge-agreement-fixture', version:'0.0.1', description:'Installer agreement contract fixture',
      author:'PhaseForge', license:'MIT', main:'index.js',
    }));
    const actual = JSON.parse(await fs.readFile(path.join(root, 'desktop/package.json'), 'utf8')).build;
    const {build, Platform} = require('electron-builder');
    const {NsisTarget} = require('app-builder-lib/out/targets/nsis/NsisTarget');
    const original = NsisTarget.prototype.executeMakensis;
    const scripts = [];
    NsisTarget.prototype.executeMakensis = async function(defines, commands, script, options) {
      const kind = Object.hasOwn(defines, 'BUILD_UNINSTALLER') ? 'uninstaller' : 'installer';
      scripts.push({kind, script});
      await fs.writeFile(path.join(output, `${kind}.nsi`), script);
      await fs.writeFile(path.join(output, `${kind}-options.json`), JSON.stringify({defines,commands}, null,2));
      if (kind === 'installer') {
        const {getMakeNsisPath} = require('app-builder-lib/out/toolsets/windows');
        const {nsisEscapeString} = require('app-builder-lib/out/targets/nsis/nsisScriptGenerator');
        const {nsisTemplatesDir} = require('app-builder-lib/out/targets/nsis/nsisUtil');
        const compiler = await getMakeNsisPath(actual.toolsets.nsis);
        const args=['-PPO','-INPUTCHARSET','UTF8'];
        for (const [key,value] of Object.entries(defines)) args.push(value==null ? `-D${key}` : `-D${key}=${nsisEscapeString(String(value))}`);
        for (const [key,value] of Object.entries(commands)) for(const entry of Array.isArray(value)?value:[value]) args.push(`-X${key} ${entry}`);
        const expanded=await require('builder-util').spawnAndWriteWithOutput(compiler.path,[...args,'-'],script,{cwd:nsisTemplatesDir,
          env:{...process.env,...compiler.env},timeout:30000,windowsHide:true});
        await fs.writeFile(path.join(output,'installer-expanded.nsi'),expanded.stdout||'');
        await fs.writeFile(path.join(output,'preprocess-stderr.log'),expanded.stderr||'');
        const pages=[...expanded.stdout.matchAll(/^PageEx license\s*\n([\s\S]*?)^PageExEnd/gmi)];
        assert.equal(pages.length,1,'exactly one compiled license page, including update entry');
        assert.match(pages[0][1],/LicenseForceSelection checkbox "I accept the Microsoft runtime terms\."/i);
        const pre=pages[0][1].match(/PageCallbacks (\S+)/i)?.[1];
        assert.ok(pre,'license page has a generated pre callback');
        const callback=expanded.stdout.match(new RegExp(`^Function "?${pre.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')}"?\\s*\\n([\\s\\S]*?)^FunctionEnd`,'mi'));
        assert.ok(callback,'generated license pre callback is present');
        assert.doesNotMatch(callback[1],/skipPageIfUpdated|isUpdated/i,'update detection cannot skip the agreement page');
      }
      return original.call(this, defines, commands, script, options);
    };
    let artifacts;
    try {
      artifacts = await build({projectDir:project, prepackaged:payload, targets:Platform.WINDOWS.createTarget('nsis', require('builder-util').Arch.x64), publish:'never', config:{
        appId:`science.phaseforge.agreement-fixture.${crypto.randomUUID()}`,
        productName:name, electronVersion:require('../package.json').devDependencies.electron,
        directories:{output:path.join(output,'artifacts')}, compression:'store',
        toolsets:actual.toolsets, win:{signAndEditExecutable:false}, publish:null,
        nsis:{...actual.nsis, include:'build/installer.nsh', license:terms,
          installerIcon:undefined, uninstallerIcon:undefined,
          createDesktopShortcut:false, createStartMenuShortcut:false, runAfterFinish:false},
      }});
    } finally { NsisTarget.prototype.executeMakensis = original; }
    assert.deepEqual(scripts.map(row=>row.kind), ['uninstaller','installer']);
    const script = scripts.find(row=>row.kind==='installer').script;
    const includeAt = script.indexOf('build\\installer.nsh');
    assert.ok(includeAt >= 0, 'actual shared header includes our hook');
    assert.ok(script.indexOf('!macro licensePage') > includeAt, 'builder defines license only after parsing the custom include');
    const exe = artifacts.find(file=>file.endsWith('.exe'));
    assert.ok(exe);
    // The real installer must reject hidden assent before installing a payload,
    // including updater entry. Full accepted-install behavior is covered by the
    // release native harness; this test does not install an application.
    const cases=[];
    for (const args of [['/S'],['/S','/ACCEPT_MSVC_TERMS=wrong'],['/S','--updated']]) {
      const destination = path.join(output, `rejected-${cases.length}`);
      const result = spawnSync(exe, [...args, `/D=${destination}`], {timeout:30000, windowsHide:true});
      assert.equal(result.error, undefined);
      assert.equal(result.status, 2, `${args.join(' ')} must reject absent or incorrect assent`);
      assert.equal(await fs.stat(path.join(destination,`${name}.exe`)).then(()=>true,()=>false), false);
      cases.push({args,status:result.status,payloadInstalled:false});
    }
    const hash = async file => crypto.createHash('sha256').update(await fs.readFile(file)).digest('hex');
    await fs.writeFile(path.join(output,'report.json'),JSON.stringify({passed:true,
      scope:'Actual electron-builder complete installer/uninstaller compilation with production hook and terms; minimal non-executable payload; rejected silent invocations only',
      builderVersion:require('electron-builder/package.json').version, exe, installerSha256:await hash(exe),
      hookSha256:await hash(path.join(project,'build/installer.nsh')),termsSha256:await hash(path.resolve(project,terms)),
      scripts:await Promise.all(scripts.map(async({kind})=>({kind,sha256:await hash(path.join(output,`${kind}.nsi`))}))),
      expandedScriptSha256:await hash(path.join(output,'installer-expanded.nsi')),cases,
    },null,2)+'\n');
    console.log(`Installer agreement integration evidence: ${output}`);
  });
