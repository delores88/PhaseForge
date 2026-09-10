"""Check generated real-JSX static layout fixtures, NOT a running Next/React app.
Requires optional developer Playwright and Chromium. See render_discovery_fixtures.cjs.
"""
from pathlib import Path
import argparse,json,shutil
from playwright.sync_api import sync_playwright

def main():
    p=argparse.ArgumentParser();p.add_argument('--folder',default='build/layout-fixtures');p.add_argument('--chromium',default=shutil.which('chromium'));a=p.parse_args()
    root=Path(a.folder);checks=[]
    with sync_playwright() as pw:
        browser=pw.chromium.launch(executable_path=a.chromium,headless=True,args=['--no-sandbox'])
        for width in [1440,1024,540]:
            for theme in ['dark','light']:
                for tab in ['campaign','record','publish']:
                    page=browser.new_page(viewport={'width':width,'height':1000})
                    page.set_content((root/f'{tab}-{theme}.html').read_text(),wait_until='domcontentloaded');page.wait_for_timeout(120)
                    overflow=page.evaluate('document.documentElement.scrollWidth > innerWidth + 2')
                    errors=page.locator('.discError').count()
                    heading={'campaign':'Behavioral diversity investigation','record':'Make every claim traceable','publish':'Take the work out of the chat'}[tab]
                    visible=page.get_by_text(heading,exact=True).first.is_visible()
                    checks.append({'width':width,'theme':theme,'tab':tab,'horizontal_overflow':overflow,'expected_heading_visible':visible,'rendered_error_banners':errors})
                    if width in (1440,540) and tab=='campaign':page.screenshot(path=str(root/f'campaign-{theme}-{width}.png'),full_page=True)
                    page.close()
        browser.close()
    (root/'layout-checks.json').write_text(json.dumps(checks,indent=2))
    failures=[r for r in checks if r['horizontal_overflow'] or not r['expected_heading_visible']]
    print(json.dumps({'fixtures_checked':len(checks),'failures':failures},indent=2))
    raise SystemExit(bool(failures))
if __name__=='__main__':main()
