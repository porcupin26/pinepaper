#!/usr/bin/env python3
"""Exercise a real Wayland session in an isolated headless Sway compositor.

Needs sway, grim, wtype, and a built binary. Never opens the user's library.
"""
import json
import math
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time


def main():
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else 'target/release/pinepaper').resolve()
    output = Path('artifacts/wayland').resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='pinepaper-wayland-') as directory:
        root = Path(directory)
        root.chmod(0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(root), WLR_BACKENDS='headless',
                   WLR_LIBINPUT_NO_DEVICES='1', WLR_RENDERER='pixman')
        env.pop('SWAYSOCK', None)
        env.pop('WAYLAND_DISPLAY', None)
        config = root / 'sway.conf'
        config.write_text('output HEADLESS-1 mode 1872x1404 scale 2\nseat seat0 fallback true\n')
        with (output / 'sway.log').open('w') as log:
            compositor = subprocess.Popen(['sway', '-c', str(config)], env=env,
                                          stdout=log, stderr=log)
            app = None
            pointer = None
            try:
                for _ in range(100):
                    sockets = list(root.glob('sway-ipc*.sock'))
                    displays = [p for p in root.glob('wayland-*') if not p.name.endswith('.lock')]
                    if sockets and displays:
                        break
                    if compositor.poll() is not None:
                        raise RuntimeError('Headless compositor failed; see sway.log')
                    time.sleep(.1)
                env.update(SWAYSOCK=str(sockets[0]), WAYLAND_DISPLAY=displays[0].name)

                def sway(command):
                    result = subprocess.check_output(['swaymsg', '-r', command], env=env, text=True)
                    assert all(r['success'] for r in json.loads(result)), result

                def key(*args):
                    subprocess.run(['wtype', '-s', '200', *args], env=env, check=True)
                    time.sleep(.2)

                pointer = subprocess.Popen([str(binary.parent / 'examples/wayland_pointer')],
                                           env=env, stdin=subprocess.PIPE, text=True)
                time.sleep(.4)

                def point(command):
                    pointer.stdin.write(command + '\n')
                    pointer.stdin.flush()
                    time.sleep(.01)

                def click(x, y, w=936, h=702):
                    point(f'move {x} {y} {w} {h}')
                    point('down')
                    point('up')
                    time.sleep(.3)

                with (output / 'app.log').open('w') as app_log:
                    app = subprocess.Popen([str(binary), '--data-dir', str(root / 'library')],
                                           env=env, stdout=app_log, stderr=app_log)
                    time.sleep(1)
                    click(450, 36)  # Notebook opens drawing directly from the library toolbar.
                    db = sqlite3.connect(root / 'library/library.sqlite3')
                    assert db.execute('SELECT title FROM documents').fetchone()[0] == 'Notebook 1'
                    click(160, 65)  # Naming is optional, through the document title.
                    point('move 52 539 936 702')  # Q on the built-in keyboard.
                    point('down')
                    time.sleep(.08)
                    subprocess.run(['grim', str(output / 'key-pressed.png')], env=env, check=True)
                    point('up')
                    time.sleep(.65)
                    subprocess.run(['grim', str(output / 'key-released.png')], env=env, check=True)
                    key('-M', 'ctrl', 'a', '-m', 'ctrl')
                    key('Wayland notebook')
                    key('-k', 'Return')
                    time.sleep(.3)
                    actual=db.execute('SELECT title FROM documents').fetchone()
                    subprocess.run(['grim', str(output / 'created.png')], env=env, check=True)
                    assert actual[0] == 'Wayland notebook', actual
                    # A virtual device advertises pointer capability on a headless seat.
                    point('move 150 240 936 702')
                    point('down')
                    for i in range(80):
                        point(f'move {150 + i * 3} {int(240 + math.sin(i / 8) * 35)} 936 702')
                    point('up')
                    time.sleep(.5)
                    payload = json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])
                    assert len(payload['strokes']) == 1, payload
                    assert len(payload['strokes'][0]['points']) > 50
                    subprocess.run(['grim', str(output / 'landscape.png')], env=env, check=True)
                    click(34, 126)
                    subprocess.run(['grim', str(output / 'pen-popup.png')], env=env, check=True)
                    click(650, 600)  # Dismiss outside; this must never ink the page.
                    assert len(json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])['strokes']) == 1
                    point('move 34 376 936 702')
                    point('down')
                    time.sleep(.7)
                    subprocess.run(['grim', str(output / 'tool-label.png')], env=env, check=True)
                    point('up')
                    time.sleep(1.1)
                    click(34, 376)  # Diagram palette.
                    subprocess.run(['grim', str(output / 'diagram-menu.png')], env=env, check=True)
                    click(340, 389)  # Arrow in the landscape popover's first row.
                    point('move 250 400 936 702')
                    point('down')
                    for i in range(30):
                        point(f'move {250+i*5} {400+i*3} 936 702')
                    point('up')
                    time.sleep(.5)
                    payload=json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])
                    assert len(payload['strokes']) == 2, payload
                    assert payload['strokes'][-1]['diagram_shape'] == 'Arrow', payload
                    subprocess.run(['grim', str(output / 'diagram.png')], env=env, check=True)
                    click(34,426)  # Select, below Diagram.
                    click(322,443)  # Select the arrow shaft.
                    subprocess.run(['grim',str(output / 'selected-arrow.png')],env=env,check=True)
                    original=payload['strokes'][-1]['points'][:2]
                    point('move 395 487 936 702')
                    point('down')
                    for i in range(1,22):
                        point(f'move {395+i*5} {487+i*3} 936 702')
                    point('up')
                    time.sleep(.4)
                    assert app.poll() is None, 'App exited while dragging an endpoint; see app.log'
                    edited=json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])['strokes'][-1]['points'][:2]
                    assert edited[0] == original[0], (edited,original)
                    assert edited[1][0] > original[1][0]+80 and edited[1][1] > original[1][1]+40
                    subprocess.run(['grim',str(output / 'edited-arrow.png')],env=env,check=True)
                    point('move 375 475 936 702')
                    point('down')
                    point('move 405 505 936 702')
                    point('up')
                    time.sleep(.4)
                    moved=json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])['strokes'][-1]['points'][:2]
                    assert abs((moved[0][0]-edited[0][0])-(moved[1][0]-edited[1][0])) < 1e-8
                    assert moved[0][0] > edited[0][0]+20 and moved[0][1] > edited[0][1]+20
                    key('-M','ctrl','z','-m','ctrl')
                    restored=json.loads(db.execute('SELECT payload FROM pages').fetchone()[0])['strokes'][-1]['points'][:2]
                    assert restored == edited
                    sway('output HEADLESS-1 transform 90')
                    time.sleep(.5)
                    subprocess.run(['grim', str(output / 'portrait.png')], env=env, check=True)
                    key('-M', 'ctrl', 'n', '-m', 'ctrl')
                    click(160, 65, 702, 936)
                    key('-M', 'ctrl', 'a', '-m', 'ctrl')
                    key('Draft żółć')
                    sway('output HEADLESS-1 transform normal')
                    time.sleep(.4)
                    subprocess.run(['grim', str(output / 'keyboard.png')], env=env, check=True)
                    key('-k', 'Return')
                    assert db.execute('SELECT count(*) FROM documents').fetchone()[0] == 2
                    assert db.execute("SELECT count(*) FROM documents WHERE title='Draft żółć'").fetchone()[0] == 1
                    key('-k', 'F11')
                    key('-k', 'F11')
                    key('-M', 'ctrl', 'q', '-m', 'ctrl')
                    assert app.wait(timeout=10) == 0
                    db.close()
                (output / 'result.json').write_text(json.dumps({'passed': True,
                    'checks': ['native Wayland launch', 'Notebook toolbar action opens drawing immediately', 'optional title rename', 'pointer ink persisted', 'pen popup and outside dismissal without ink',
                               'hold tool label', 'diagram palette and persisted arrow', 'Select and endpoint handles', 'move arrow body and undo', 'rotation', 'draft preserved across rotation', 'fullscreen', 'clean close']}, indent=2))
                print('Headless Wayland checks passed')
            finally:
                if pointer:
                    pointer.stdin.close()
                    pointer.wait(timeout=10)
                if app and app.poll() is None:
                    app.terminate()
                    app.wait(timeout=10)
                compositor.terminate()
                compositor.wait(timeout=10)


if __name__ == '__main__':
    main()
