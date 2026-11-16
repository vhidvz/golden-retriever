#!/usr/bin/env python3
"""Spike probe, run by run.sh inside the isolated headless desktop.

Questions:
  1. Which applications show their insides on the accessibility bus, and how much?
  2. What can be observed of real input (from the stand-in user in hands/)?
  3. Can steps be done again through the bus alone, and their outcome checked?
  4. Are the paths to controls stable across restarts and languages?
  5. Can the bus press keys or click on Wayland at all?

Writes out/results.json, out/probe.log and out/tree-*.txt. Text typed in the tests is
synthetic; even so, only lengths of text changes are logged, never their contents.
"""
import json, os, subprocess, sys, time, traceback
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, GLib

OUT = os.path.join(os.environ['SPIKE_DIR'], 'out')
HANDS = os.environ['SPIKE_HANDS']
HOME = os.environ['SPIKE_HOME']
ONLY = set(sys.argv[1:])
results = {}
logfile = open(os.path.join(OUT, 'probe.log'), 'w')


def log(*words):
    print(f'{time.monotonic():9.2f}', *words, file=logfile, flush=True)


def note(key, value):
    results[key] = value
    log('RESULT', key, json.dumps(value, ensure_ascii=False))


def pump(seconds=0.0):
    ctx = GLib.MainContext.default()
    end = time.monotonic() + seconds
    while True:
        while ctx.pending():
            ctx.iteration(False)
        if time.monotonic() >= end:
            return
        time.sleep(0.01)


# --- the stand-in user ---------------------------------------------------------------

_seq = 0


def hands(*words, timeout=15):
    global _seq
    _seq += 1
    with open(f'{HANDS}/cmd.tmp', 'w') as f:
        json.dump({'seq': _seq, 'do': list(words)}, f)
    os.replace(f'{HANDS}/cmd.tmp', f'{HANDS}/cmd')
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        pump(0.05)
        try:
            with open(f'{HANDS}/done') as f:
                answer = json.load(f)
        except (OSError, ValueError):
            continue
        if answer['seq'] == _seq:
            if 'error' in answer:
                raise RuntimeError(answer['error'])
            return answer['result']
    raise TimeoutError(f'hands: {words}')


def window_of(pid):
    for w in hands('windows'):
        if w['pid'] == pid:
            return w
    return None


# --- the accessibility bus -----------------------------------------------------------

def desktop_apps():
    d = Atspi.get_desktop(0)
    out = []
    for i in range(d.get_child_count()):
        a = d.get_child_at_index(i)
        if a is not None:
            out.append(a)
    return out


def safe(f, default=None):
    try:
        return f()
    except Exception:
        return default


def walk(root, limit=40000, seconds=60):
    """Every node under root, depth first, with its depth and path of child indexes."""
    stack = [(root, 0, ())]
    start = time.monotonic()
    count = 0
    while stack and count < limit and time.monotonic() - start < seconds:
        acc, depth, path = stack.pop()
        count += 1
        yield acc, depth, path
        n = safe(acc.get_child_count, 0)
        for i in reversed(range(min(n, 2000))):
            child = safe(lambda: acc.get_child_at_index(i))
            if child is not None:
                stack.append((child, depth + 1, path + (i,)))


def role(acc):
    return safe(acc.get_role_name, '?')


def name(acc):
    return safe(acc.get_name, '') or ''


def ifaces(acc):
    return safe(acc.get_interfaces, []) or []


def states(acc):
    s = safe(acc.get_state_set)
    return set() if s is None else {Atspi.StateType(t).value_nick for t in s.get_states()}


def find(root, test, limit=40000):
    for acc, depth, path in walk(root, limit):
        if safe(lambda: test(acc), False):
            return acc
    return None


def find_all(root, test, limit=40000):
    return [acc for acc, d, p in walk(root, limit) if safe(lambda: test(acc), False)]


def actions(acc):
    if 'Action' not in ifaces(acc):
        return []
    return [Atspi.Action.get_action_name(acc, i) for i in range(Atspi.Action.get_n_actions(acc))]


def do(acc, wanted=('click', 'activate', 'press', 'toggle')):
    names = actions(acc)
    for i, n in enumerate(names):
        if n in wanted:
            return Atspi.Action.do_action(acc, i)
    if names:
        return Atspi.Action.do_action(acc, 0)
    raise RuntimeError(f'no action on {role(acc)} {name(acc)!r}')


def text_of(acc):
    if 'Text' not in ifaces(acc):
        return None
    return Atspi.Text.get_text(acc, 0, Atspi.Text.get_character_count(acc))


def path_to(acc, stop):
    """(role, name, index among siblings) from stop down to acc."""
    steps = []
    while acc is not None and not safe(lambda: acc == stop, False):
        parent = safe(acc.get_parent)
        steps.append((role(acc), name(acc), safe(acc.get_index_in_parent, -1)))
        if parent is None or role(acc) == 'application':
            break
        acc = parent
    return list(reversed(steps))


def wait_app(match, pid=None, timeout=40):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        for a in desktop_apps():
            if (pid and safe(a.get_process_id) == pid) or match(name(a).lower()):
                if safe(a.get_child_count, 0) > 0:
                    return a
        pump(0.25)
    return None


# --- launching -----------------------------------------------------------------------

APPS = {
    'calculator': (['gnome-calculator'], lambda n: 'calculator' in n),
    'editor': (['gnome-text-editor', '--new-window'], lambda n: 'text-editor' in n or 'texteditor' in n),
    'files': (['nautilus', '--new-window', HOME], lambda n: 'nautilus' in n),
    'firefox': (['firefox', '--no-remote', '--new-instance',
                 'data:text/html,<title>Form</title><label>Search <input id=q></label><button onclick="document.title=1">Go</button>'],
                lambda n: n == 'firefox'),
    'writer': (['libreoffice', '--writer', '--norestore', '--nologo'], lambda n: 'soffice' in n or 'libreoffice' in n),
    'code': (['code', '--ozone-platform=wayland', '--user-data-dir', f'{HOME}/code', '--disable-extensions',
              '--new-window', '--disable-workspace-trust', '--skip-welcome'] + (['--force-renderer-accessibility'] if os.environ.get('FORCE') else []), lambda n: n in ('code', 'visual studio code', 'electron')),
}
running = {}


def launch(key, extra_env=None):
    cmd, match = APPS[key]
    env = dict(os.environ, GDK_BACKEND='wayland', **(extra_env or {}))
    if os.environ.get('FORCE'):
        env['ACCESSIBILITY_ENABLED'] = '1'
    started = time.monotonic()
    proc = subprocess.Popen(cmd, env=env, stdout=open(os.path.join(OUT, f'app-{key}.log'), 'w'), stderr=subprocess.STDOUT)
    running[key] = proc
    app = wait_app(match, proc.pid)
    log('launched', key, 'pid', proc.pid, 'found' if app else 'NOT FOUND', f'{time.monotonic() - started:.1f}s')
    return app


def stop(key):
    proc = running.pop(key, None)
    if proc:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
    pump(1)


# --- 1. what each application shows ----------------------------------------------------

def survey(key, app):
    start = time.monotonic()
    nodes, depth, roles, named, with_id = 0, 0, {}, 0, 0
    counts = {'Action': 0, 'Text': 0, 'EditableText': 0, 'Value': 0, 'Selection': 0, 'Component': 0}
    lines = []
    for acc, d, path in walk(app):
        nodes += 1
        depth = max(depth, d)
        r = role(acc)
        roles[r] = roles.get(r, 0) + 1
        n = name(acc)
        named += bool(n)
        ident = safe(acc.get_accessible_id, '') or ''
        with_id += bool(ident)
        fs = ifaces(acc)
        for i in counts:
            counts[i] += i in fs
        if len(lines) < 4000:
            acts = ','.join(actions(acc)) if 'Action' in fs else ''
            lines.append(f'{"  " * d}{r} {n[:60]!r}' + (f' id={ident}' if ident else '') + (f' [{acts}]' if acts else '')
                         + (' editable' if 'EditableText' in fs else ''))
    with open(os.path.join(OUT, f'tree-{key}.txt'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    top = sorted(roles.items(), key=lambda kv: -kv[1])[:8]
    note(f'survey.{key}', {'nodes': nodes, 'depth': depth, 'named': named, 'with_id': with_id, 'interfaces': counts,
                           'roles': dict(top), 'seconds': round(time.monotonic() - start, 2),
                           'toolkit': f'{safe(app.get_toolkit_name, "?")} {safe(app.get_toolkit_version, "")}'})


def step(title, f, *args):
    if ONLY and not ONLY & set(title.split('.')):
        return
    log('==', title)
    try:
        f(*args)
    except Exception as e:
        note(f'{title}.error', f'{type(e).__name__}: {e}')
        log(traceback.format_exc())


def main():
    Atspi.init()
    pump(1)
    note('a11y.enabled', os.environ.get('A11Y'))
    note('desktop.apps_at_start', [name(a) for a in desktop_apps()])
    for key in APPS:
        step(f'survey.{key}', lambda k=key: survey_one(k))
    with open(os.path.join(OUT, 'results.json'), 'w') as f:
        json.dump(results, f, indent=1, ensure_ascii=False)
    hands('quit')


def survey_one(key):
    app = launch(key)
    if app is None:
        note(f'survey.{key}', 'not on the accessibility bus')
        log('apps now:', [name(a) for a in desktop_apps()])
    else:
        pump(float(os.environ.get('SETTLE', '2')))
        log('apps after settling:', [(name(a), safe(a.get_process_id), safe(a.get_child_count)) for a in desktop_apps()])
        log('processes:', subprocess.run(['pgrep', '-af', 'code|electron'], capture_output=True, text=True).stdout[-1500:])
        app = wait_app(APPS[key][1]) or app   # some (VS Code) restart themselves once
        survey(key, app)
    stop(key)


if __name__ == '__main__':
    main()
