"""Chromium static layout check of actual JSX fixtures; not Next hydration."""
import argparse,json,shutil
from pathlib import Path
from playwright.sync_api import sync_playwright
p=argparse.ArgumentParser();p.add_argument('--folder',required=True);p.add_argument('--browser');a=p.parse_args();root=Path(a.folder);results=[]
with sync_playwright() as pw:
    executable=a.browser or shutil.which('chromium') or shutil.which('google-chrome')
    browser=pw.chromium.launch(headless=True,executable_path=executable,args=['--no-sandbox','--disable-dev-shm-usage'])
    for width,host in [(1440,None),(1024,None),(540,None),(1440,560)]:
        for theme in ['dark','light']:
            for scenario in ['empty','populated','error']:
                page=browser.new_page(viewport={'width':width,'height':1000})
                page.set_content((root/f'{scenario}-{theme}.html').read_text(),wait_until='domcontentloaded')
                if host:page.evaluate('(w)=>document.documentElement.style.setProperty("--fixture-width",w+"px")',host)
                page.wait_for_timeout(60)
                overflow=page.evaluate('document.documentElement.scrollWidth > innerWidth+2')
                local=page.evaluate('document.querySelector(".fixtureHost").scrollWidth>document.querySelector(".fixtureHost").clientWidth+2')
                heading=page.get_by_text('Notes, sources & research history',exact=True).is_visible()
                content=page.get_by_text('Compare competing explanations',exact=True).is_visible() if scenario!='empty' else page.get_by_text('A missing solver need not end the investigation',exact=True).is_visible()
                advice=page.get_by_text('Available system RAM',exact=True).is_visible()
                results.append(dict(width=width,host=host,theme=theme,scenario=scenario,overflow=overflow,local_overflow=local,heading=heading,content=content,advice=advice))
                if scenario=='populated' and width in (1440,540):page.screenshot(path=str(root/f'{scenario}-{theme}-{width}-{host or "full"}.png'),full_page=True)
                page.close()
    browser.close()
(root/'layout-checks.json').write_text(json.dumps(results,indent=2))
failures=[r for r in results if r['overflow'] or r['local_overflow'] or not r['heading'] or not r['content'] or not r['advice']]
print(json.dumps({'fixtures':len(results),'failures':failures},indent=2));raise SystemExit(bool(failures))
