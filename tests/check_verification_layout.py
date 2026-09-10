"""Actual JSX + CSS static layout fixture inspection. NOT live React/Next or a backend test."""
from pathlib import Path
import argparse,json,shutil
from playwright.sync_api import sync_playwright

def main():
    p=argparse.ArgumentParser();p.add_argument('--folder',default='build/verification-fixtures');p.add_argument('--chromium',default=shutil.which('chromium'));a=p.parse_args()
    root=Path(a.folder);checks=[]
    headings={'prepare':'Choose exactly what needs to survive','evidence':'What actually survived?','prior':'Nearest documented numerical results','review':'Make the comparison accountable'}
    with sync_playwright() as pw:
        browser=pw.chromium.launch(executable_path=a.chromium,headless=True,args=['--no-sandbox'])
        for width in (1440,1024,540):
            for theme in ('dark','light'):
                for tab in ('prepare','evidence','prior','review'):
                    page=browser.new_page(viewport={'width':width,'height':1000})
                    page.set_content((root/f'{tab}-{theme}.html').read_text(),wait_until='domcontentloaded');page.wait_for_timeout(90)
                    overflow=page.evaluate('document.documentElement.scrollWidth > innerWidth + 2')
                    visible=page.get_by_text(headings[tab],exact=True).first.is_visible()
                    checks.append({'width':width,'theme':theme,'tab':tab,'horizontal_overflow':overflow,'expected_heading_visible':visible})
                    if width in (1440,540):page.screenshot(path=str(root/f'{tab}-{theme}-{width}.png'),full_page=True)
                    page.close()
        browser.close()
    (root/'layout-checks.json').write_text(json.dumps(checks,indent=2))
    failures=[r for r in checks if r['horizontal_overflow'] or not r['expected_heading_visible']]
    print(json.dumps({'fixtures_checked':len(checks),'failures':failures},indent=2));raise SystemExit(bool(failures))
if __name__=='__main__':main()
