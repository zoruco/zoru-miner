#!/usr/bin/env python3
"""Exercise guided setup and real PTY terminal restoration on a disposable EVM."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import termios
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--options', required=True, type=Path)
    parser.add_argument('--cli', required=True, type=Path)
    args = parser.parse_args()
    cfg = json.loads(args.options.read_text())
    base = args.options.parent/'terminal'
    base.mkdir(mode=0o700)
    env = dict(os.environ, TERM='xterm-256color', LC_ALL='C.UTF-8',
               XDG_CONFIG_HOME=str(base/'config'), ZORU_STATE_ROOT=str(base/'signing'))
    request = urllib.request.Request(cfg['read'], json.dumps({'jsonrpc':'2.0','id':1,
        'method':'web3_clientVersion','params':[]}).encode(), {'Content-Type':'application/json'})
    with urllib.request.urlopen(request) as response:
        assert 'anvil' in json.load(response)['result'].lower()
    wallet = json.loads(Path(cfg['wallet']).read_text())
    account = '0x'+wallet['address'].removeprefix('0x')

    def tty(arguments, actions, timeout=45):
        master, slave = pty.openpty()
        before = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 35, 110, 0, 0))
        process = subprocess.Popen([str(args.cli), *arguments], stdin=slave, stdout=slave, stderr=slave, env=env)
        output = bytearray()
        started = time.monotonic()
        try:
            while process.poll() is None:
                assert time.monotonic()-started < timeout, 'Terminal fixture timed out'
                if select.select([master], [], [], 0.05)[0]:
                    output.extend(os.read(master, 65536))
                    assert len(output)<2_000_000, 'Unexpected terminal output flood'
                actions(process, master, slave, output)
            while select.select([master], [], [], 0)[0]:
                output.extend(os.read(master, 65536))
            assert termios.tcgetattr(slave)==before, 'Terminal attributes were not restored'
            return process.returncode, bytes(output)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)

    sent = False
    def interrupted(_process, master, _slave, output):
        nonlocal sent
        if b'Trusted deployment manifest' in output and not sent:
            os.write(master,b'\x04')
            sent=True
    code, output = tty(['setup'],interrupted)
    assert code!=0 and b'Setup interrupted' in output
    config_path = base/'config/zoru-miner/config.json'
    assert not config_path.exists(), 'Interrupted setup wrote configuration'
    sent=False
    answers=[cfg['manifest'],cfg['wallet'],cfg['read'],str(base/'state'),'2','1','0']
    def setup(_process,master,_slave,output):
        nonlocal sent
        if b'Trusted deployment manifest' in output and not sent:
            os.write(master, ('\n'.join(answers)+'\n').encode())
            sent=True
    code, output = tty(['setup'],setup)
    assert code==0,output[-2000:]
    saved=json.loads(config_path.read_text())
    assert (saved['workers'],saved['cpu_budget'],saved['auto_claim_after'])==(2,1.0,0)
    assert 'password' not in saved and 'private_key' not in saved
    original=config_path.read_bytes()
    repeated=subprocess.run([str(args.cli),'setup'],env=env,capture_output=True)
    assert repeated.returncode!=0 and config_path.read_bytes()==original
    mined=subprocess.run([str(args.cli),'mine','--password-stdin','--seconds','5'],input=cfg['password']+'\n',
                         text=True,capture_output=True,env=env,timeout=45)
    assert mined.returncode==0,mined.stdout[-3000:]
    frames=[json.loads(line) for line in mined.stdout.splitlines()]
    result=next(f['data'] for f in frames if f.get('event')=='mine-result')
    assert result['workers']==2 and result['cpuBudgetCores']==1.0
    assert '\x1b' not in mined.stdout,'Redirected mining output contains terminal escapes'
    # Subsequent read-only runs use an account override and do not unlock or sign.
    overrides=subprocess.run([str(args.cli),'mine','--account',account,'--dry-run','--plain','--seconds','2',
                              '--workers','1','--cpu-budget','0.1'],text=True,capture_output=True,env=env,timeout=45)
    assert overrides.returncode==0,overrides.stdout[-2000:]
    result=next(json.loads(line)['data'] for line in overrides.stdout.splitlines() if json.loads(line).get('event')=='mine-result')
    assert result['workers']==1 and result['cpuBudgetCores']==0.1
    assert config_path.read_bytes()==original
    for ending in ['q',signal.SIGINT,signal.SIGTERM]:
        active=None
        resized=False
        stopped=False
        def controls(process, master, slave, output):
            nonlocal active,resized,stopped
            if b'\x1b[?1049h' in output and active is None:
                active=time.monotonic()
            if active is not None and time.monotonic()-active>0.2 and not resized:
                fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',16,42,0,0))
                process.send_signal(signal.SIGWINCH)
                os.write(master,b'\x1b[A\x1b[5~lR')
                resized=True
            if active is not None and time.monotonic()-active>(0.7 if ending==signal.SIGTERM else 3) and not stopped:
                if ending=='q':
                    os.write(master,b'q')
                else:
                    process.send_signal(ending)
                stopped=True
        code,output=tty(['mine','--account',account,'--dry-run','--seconds','30','--ascii'],controls)
        assert code==0,output[-3000:]
        assert active is not None and resized and stopped
        assert b'ZORU MINER' in output and b'\x1b[?1049l' in output
        final = json.JSONDecoder().raw_decode(output[output.rfind(b'{"data":'):].decode())[0]
        assert final['data']['errors']==[], 'Graceful cancellation became a mining error'
    report={'status':'PASS','guidedSetup':True,'interruptedSetup':True,'savedMining':True,
            'overrides':True,'nonTtyJson':True,'ttyResizeAndScroll':True,'q':True,'sigint':True,'sigterm':True,
            'terminalRestored':True,'configPreserved':True}
    (base/'report.json').write_text(json.dumps(report))
    print(json.dumps(report))


if __name__=='__main__':
    main()
