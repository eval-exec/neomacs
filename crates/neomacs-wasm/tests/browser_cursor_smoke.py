#!/usr/bin/env python3
"""Verify mouse point placement and visible cursor animation in packaged Chrome."""
import argparse
import time
from selenium import webdriver
from browser_test_support import BrowserEditorHarness, chrome_options

def cursor_pixels(driver, cursor):
    return driver.execute_async_script(r"""
      const [png, cursor, done] = arguments;
      const image = new Image();
      image.onload = () => {
        const bounds = document.querySelector('canvas').getBoundingClientRect();
        const raster = document.createElement('canvas');
        raster.width = image.width; raster.height = image.height;
        const ctx = raster.getContext('2d'); ctx.drawImage(image, 0, 0);
        const sx = image.width / bounds.width, sy = image.height / bounds.height;
        done(Array.from(ctx.getImageData(Math.floor((cursor.x + cursor.width/2)*sx),
          Math.floor((cursor.y + cursor.height/2)*sy), 1, 1).data));
      };
      image.src = 'data:image/png;base64,' + png;
    """, driver.find_element('css selector', 'canvas').screenshot_as_base64, cursor)

def cursor_x_on_empty_line(driver, cursor):
    return driver.execute_async_script(r"""
      const [png, cursor, done] = arguments;
      const image = new Image();
      image.onload = () => {
        const bounds = document.querySelector('canvas').getBoundingClientRect();
        const raster = document.createElement('canvas');
        raster.width = image.width; raster.height = image.height;
        const ctx = raster.getContext('2d'); ctx.drawImage(image, 0, 0);
        const sx = image.width / bounds.width, sy = image.height / bounds.height;
        const pixels = ctx.getImageData(0, Math.floor((cursor.y+cursor.height/2)*sy), image.width, 1).data;
        // This row contains only spaces. Compare with its background, not a
        // saturation threshold: cycling also passes through muted colors.
        const background = Array.from(pixels.slice(Math.floor(image.width*0.8)*4, Math.floor(image.width*0.8)*4+3));
        for (let x=0; x<image.width*0.75; x++) {
          const rgb = Array.from(pixels.slice(x*4, x*4+3));
          if (rgb.some((value, channel) => Math.abs(value-background[channel])>20)) { done(x/sx); return; }
        }
        done(null);
      };
      image.src = 'data:image/png;base64,' + png;
    """, driver.find_element('css selector', 'canvas').screenshot_as_base64, cursor)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:4173/')
    parser.add_argument('--chrome')
    parser.add_argument('--headless', action='store_true')
    parser.add_argument('--scale', type=float, default=1.0)
    parser.add_argument('--mode', choices=['mouse', 'color', 'motion'], required=True)
    args = parser.parse_args()
    options = chrome_options(args.chrome, args.headless)
    # Use actual device scaling. CDP metrics emulation can change DPR without
    # updating the device-pixel ResizeObserver size that winit uses.
    options.add_argument(f'--force-device-scale-factor={args.scale}')
    options.add_argument('--window-size=1100,800')
    driver = webdriver.Chrome(options=options)
    editor = BrowserEditorHarness(driver, 30)
    try:
        editor.install_frame_observer(); driver.get(args.url)
        editor.wait_ready(); editor.wait_for_presentation()
        geometry = driver.execute_script(r"""
          const canvas = document.querySelector('canvas');
          const bounds = canvas.getBoundingClientRect();
          return {width: canvas.width, height: canvas.height,
            expectedWidth: bounds.width*devicePixelRatio,
            expectedHeight: bounds.height*devicePixelRatio};
        """)
        assert abs(geometry['width']-geometry['expectedWidth']) < 2, geometry
        assert abs(geometry['height']-geometry['expectedHeight']) < 2, geometry
        if args.mode == 'mouse':
            editor.eval_expression(r'''(progn (switch-to-buffer "*scratch*") (erase-buffer)
              (insert "first line\nsecond line\nthird line") (goto-char (point-max))
              (message (concat "MOUSE-" "READY")))''', 'MOUSE-READY')
            bounds = driver.execute_script('return document.querySelector("canvas").getBoundingClientRect().toJSON()')
            for event in ['mousePressed', 'mouseReleased']:
                driver.execute_cdp_cmd('Input.dispatchMouseEvent', dict(type=event, x=bounds['x']+40, y=bounds['y']+12, button='left', clickCount=1))
            time.sleep(0.3)
            editor.eval_expression(r'''(message (concat "CLICK-" (if (= (line-number-at-pos) 1) "PASS" "FAIL")))''', 'CLICK-PASS', 'CLICK-FAIL')
            editor.eval_expression(r'''(progn (setq wasm-mouse-left-window (selected-window))
              (split-window-right) (message (concat "SPLIT-" "READY")))''', 'SPLIT-READY')
            time.sleep(0.2)
            for event in ['mousePressed', 'mouseReleased']:
                driver.execute_cdp_cmd('Input.dispatchMouseEvent', dict(type=event, x=bounds['x']+bounds['width']*0.75, y=bounds['y']+12, button='left', clickCount=1))
            time.sleep(0.2)
            editor.eval_expression(r'''(message (concat "WINDOW-" (if (eq (selected-window) wasm-mouse-left-window) "FAIL" "PASS")))''', 'WINDOW-PASS', 'WINDOW-FAIL')
        elif args.mode == 'color':
            editor.eval_expression(r'''(progn (erase-buffer) (message (concat "COLOR-" "READY")))''', 'COLOR-READY')
            editor.click_editor_canvas()  # Hidden text service, not canvas, owns keyboard focus.
            time.sleep(0.5)
            cursor = editor.assert_active_cursor('empty scratch buffer')
            colors = []
            for _ in range(8):
                colors.append(cursor_pixels(driver, cursor))
                time.sleep(0.15)
            assert len({tuple(color) for color in colors}) >= 3, f'cursor color did not animate: {colors}, cursor={cursor}'
        else:
            editor.eval_expression(r'''(progn (erase-buffer) (insert (make-string 40 32))
              (goto-char 1) (message (concat "MOTION-" "READY")))''', 'MOTION-READY')
            time.sleep(0.5)
            start = editor.assert_active_cursor('motion start')
            for kind in ['keyDown', 'keyUp']:
                driver.execute_cdp_cmd('Input.dispatchKeyEvent', dict(type=kind, key='End', code='End', windowsVirtualKeyCode=35))
            positions = []
            for _ in range(8):
                positions.append(cursor_x_on_empty_line(driver, start))
                time.sleep(0.03)
            target = editor.assert_active_cursor('motion destination')
            assert target['x'] > start['x'] + 100, f'End did not move the logical cursor: {start}, {target}'
            assert any(x is not None and start['x']+5 < x < target['x']-5 for x in positions), f'no intermediate cursor pixels: {positions}, target={target}'
        print(f'PASS: browser cursor {args.mode}')
    finally:
        driver.quit()

if __name__ == '__main__':
    main()
