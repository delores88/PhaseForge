"""Chromium layout checks on real JSX output. NOT a hydrated Next application."""
from pathlib import Path
import argparse,json,shutil
from playwright.sync_api import sync_playwright
p=argparse.ArgumentParser();p.add_argument('--folder',required=True);p.add_argument('--browser');a=p.parse_args();root=Path(a.folder);checks=[]
with sync_playwright() as pw:
 browser=pw.chromium.launch(executable_path=a.browser or shutil.which('chromium'),headless=True,args=['--no-sandbox','--disable-dev-shm-usage'])
 for width,host in [(1440,None),(1024,None),(540,None),(1440,560)]:
  for theme in ['dark','light']:
   for scenario in ['empty','request','ready','running','results','error','manual']:
    page=browser.new_page(viewport={'width':width,'height':1000});page.set_default_timeout(1500)
    page.set_content((root/f'{scenario}-{theme}.html').read_text(),wait_until='domcontentloaded')
    if host:page.evaluate('(w)=>document.documentElement.style.setProperty("--fixture-width",w+"px")',host)
    page.wait_for_timeout(30)
    checks_row={'width':width,'host':host,'theme':theme,'scenario':scenario,
       'document_overflow':page.evaluate('document.documentElement.scrollWidth>innerWidth+2'),
       'pane_overflow':page.evaluate('document.querySelector(".fixtureHost").scrollWidth>document.querySelector(".fixtureHost").clientWidth+2'),
       'no_discovery_navigation':page.locator('a[href="/discovery/"]').count()==0,
       'no_stage_gate_ribbon':page.locator('.workflowBar ol').count()==0}
    label={'empty':'New project','request':'Build & run','ready':'Run saved setup','running':'Stop','results':'View results','error':'Build & run','manual':'Save executable setup'}[scenario]
    button=page.get_by_role('button',name=label,exact=True)
    checks_row['primary_action_visible']=button.is_visible()
    checks_row['primary_action_enabled']=button.is_enabled() if button.count() else False
    if width in [1440,540] and not host and scenario in ['request','ready','running','manual']:
     page.screenshot(path=str(root/f'{scenario}-{theme}-{width}.png'),full_page=True)
    checks.append(checks_row);page.close()
 browser.close()
(root/'layout-checks.json').write_text(json.dumps(checks,indent=2))
fail=[c for c in checks if c['document_overflow'] or c['pane_overflow'] or not all(c[k] for k in ['no_discovery_navigation','no_stage_gate_ribbon','primary_action_visible','primary_action_enabled'])]
print(json.dumps({'checks':len(checks),'failures':fail},indent=2));raise SystemExit(bool(fail))
