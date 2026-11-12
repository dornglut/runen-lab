"""Bounded actual-Chromium proof for the D3 inspector's browser interactions.

Uses about:blank with in-memory script injection so the test is independent of
browser network policies; the Node server tests cover the HTTP asset contract.
Requires Playwright 1.57.0 and a Playwright-compatible Chromium installation.
"""
from __future__ import annotations

import base64
import os
from pathlib import Path
import re

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def module_url(source: str) -> str:
    return "data:text/javascript;base64," + base64.b64encode(source.encode()).decode()


def launch_page(browser, *, reduced_motion: bool = False):
    page = browser.new_page(viewport={"width": 1440, "height": 900}, accept_downloads=True)
    page_errors = []
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    if reduced_motion:
        page.emulate_media(reduced_motion="reduce")
    html = (ROOT / "index.html").read_text()
    html, d3_count = re.subn(r'<script defer src="\./vendor/d3\.v7\.9\.0\.min\.js"[^>]*></script>', '', html)
    html, module_count = re.subn(r'<script type="module" src="\./src/main\.mjs"></script>', '', html)
    assert d3_count == module_count == 1, "HTML must load the exact local vendor and app scripts"
    page.set_content(html, wait_until="domcontentloaded")
    page.add_style_tag(content=(ROOT / "styles.css").read_text())
    page.add_script_tag(content=(ROOT / "vendor/d3.v7.9.0.min.js").read_text())
    assert page.evaluate("d3.version") == "7.9.0"
    source = (ROOT / "src/main.mjs").read_text()
    for name in ("graph-data.mjs", "sample.mjs"):
        source = source.replace(f"'./{name}'", f"'{module_url((ROOT / 'src' / name).read_text())}'")
    assert "from './" not in source, "Unexpected unbundled relative import"
    page.evaluate("async source => { await import(source) }", module_url(source))
    assert not page_errors, page_errors
    return page, page_errors


def main() -> None:
    with sync_playwright() as playwright:
        browser_path = os.environ.get("PCG_BROWSER_EXECUTABLE")
        browser = playwright.chromium.launch(headless=True, **({"executable_path": browser_path} if browser_path else {}))
        page, errors = launch_page(browser)
        assert [page.locator(f"#{key}").inner_text() for key in
                ("node-count", "edge-count", "cycle-count", "branch-count")] == ["8", "11", "4", "4"]
        assert page.locator("circle.graph-node").count() == 8
        assert page.locator("line.graph-edge").count() == 11
        assert page.locator("#status").inner_text().startswith("Simulating")
        assert not errors, errors

        # The graph is interactive, including keyboard access.
        node = page.locator('circle.graph-node[aria-label^="Node 0,"]')
        node.click()
        assert page.locator("#selection-title").inner_text() == "Node 0"
        assert "Degree:" in page.locator("#selection-body").inner_text()
        node.focus()
        node.press("p")
        assert "Pinned node 0" in page.locator("#status").inner_text()
        assert page.locator("circle.node-halo.pinned").count() == 1
        node.press("P")
        assert page.locator("circle.node-halo.pinned").count() == 0

        # Real pointer drag and zoom, rather than only direct event dispatch.
        before = node.get_attribute("cx")
        bounds = node.bounding_box()
        assert bounds is not None
        x, y = bounds["x"] + bounds["width"] / 2, bounds["y"] + bounds["height"] / 2
        page.mouse.move(x, y)
        page.mouse.down()
        page.mouse.move(x + 80, y + 30, steps=8)
        page.mouse.up()
        assert node.get_attribute("cx") != before, "Pointer drag did not move node"
        transform_before = page.locator("g.graph-viewport").get_attribute("transform")
        page.mouse.wheel(0, -320)
        page.wait_for_timeout(500)
        assert page.locator("g.graph-viewport").get_attribute("transform") != transform_before

        page.locator("#charge").fill("-400")
        assert page.locator("#charge-value").inner_text() == "-400"
        page.locator("#toggle-button").click()
        assert page.locator("#toggle-button").inner_text() == "Resume motion"
        assert page.locator("#status").inner_text() == "Simulation paused"
        page.locator("#toggle-button").click()
        assert page.locator("#toggle-button").inner_text() == "Pause motion"
        page.locator("#show-labels").uncheck()
        assert page.locator("text.node-label.hidden").count() == 8

        # Accepted JSON import and rejected malformed JSON do not corrupt the graph.
        fixture = ROOT / "fixtures/sample.json"
        page.locator("#file-input").set_input_files(fixture)
        assert page.locator("#node-count").inner_text() == "8"
        page.locator("#file-input").set_input_files({"name": "bad.json", "mimeType": "application/json", "buffer": b"{"})
        assert page.locator("#error-banner").is_visible()
        assert page.locator("#status").inner_text() == "Input rejected"
        assert page.locator("#node-count").inner_text() == "8"

        with page.expect_download() as download_event:
            page.locator("#export-button").click()
        download = download_event.value
        assert download.suggested_filename == "runen-pcg-graph-42.svg"
        svg_path = download.path()
        svg_text = svg_path.read_text()
        assert svg_text.count('class="graph-node') == 8
        assert svg_text.count('class="graph-edge') == 11
        assert not errors, errors
        page.close()

        # Reduced-motion preference must yield a stable, non-animated layout.
        reduced, errors2 = launch_page(browser, reduced_motion=True)
        assert "Static layout" in reduced.locator("#status").inner_text()
        assert reduced.locator("#toggle-button").inner_text() == "Resume motion"
        positions = reduced.locator("circle.graph-node").evaluate_all("nodes => nodes.map(n => [n.getAttribute('cx'), n.getAttribute('cy')])")
        reduced.wait_for_timeout(250)
        after = reduced.locator("circle.graph-node").evaluate_all("nodes => nodes.map(n => [n.getAttribute('cx'), n.getAttribute('cy')])")
        assert positions == after, "Reduced-motion layout changed while idle"
        assert not errors2, errors2
        reduced.close()
        browser.close()
    print("Browser smoke passed: rendering, metrics, selection, pin, drag, zoom, sliders, pause, labels, import, errors, SVG export, reduced motion")


if __name__ == "__main__":
    main()
