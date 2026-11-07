#!/usr/bin/env python3
"""Disposable, loopback-only EVM fixture using the reviewed protocol sources."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import time
import urllib.request


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contracts', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--baseline', action='store_true')
    parser.add_argument('--gui-build', type=Path)
    parser.add_argument('--tests', nargs='*', default=['recovery', 'mining_limit', 'automatic_claim', 'claim_reorg', 'continuous'])
    parser.add_argument('--terminal', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    output.chmod(0o700)
    env = dict(os.environ, PATH=str(Path.home()/'.cargo/bin')+':'+os.environ['PATH'], CARGO_BUILD_JOBS='1',
               ZORU_STATE_ROOT=str(output/'signing'), XDG_CONFIG_HOME=str(output/'config'))
    cli = root/'miner/target/release/zoru-miner-cli'

    def command(*parts):
        return subprocess.check_output([str(x) for x in parts], text=True, env=env).strip()

    def wait_for_headroom():
        if not hasattr(os, 'getloadavg'):
            return
        deadline = time.monotonic()+600
        while os.getloadavg()[0] >= (os.cpu_count() or 1)*0.95:
            if time.monotonic() >= deadline:
                raise RuntimeError('Host load leaves insufficient headroom for isolated mining; production resource guards were not bypassed')
            time.sleep(1)

    if not args.baseline:
        with (output/'tests-build.log').open('w') as build_log:
            subprocess.run(['cargo', 'test', '--release', '--locked', '--manifest-path',
                            str(root/'miner/Cargo.toml'), '--no-run'], env=env,
                           stdout=build_log, stderr=build_log, check=True)
        wait_for_headroom()

    sources = args.contracts.resolve()/'contracts'
    artifacts = output/'artifacts'
    with (output/'contracts-build.log').open('w') as log:
        subprocess.run(['forge', 'build', '--root', str(sources), '--out', str(artifacts),
                        '--cache-path', str(output/'forge-cache'),
                        'src/ZoruLedger.sol', 'src/ZoruKeccakEngine.sol',
                        'src/ZoruAdminAdapter.sol', 'test/DeepTestHarness.sol'],
                       env=env, stdout=log, stderr=log, check=True)
    url = f'http://127.0.0.1:{port()}'
    log = (output/'anvil.log').open('w')
    anvil = subprocess.Popen(['anvil', '--host', '127.0.0.1', '--port', url.rsplit(':', 1)[1],
                             '--chain-id', '6736', '--block-time', '1', '--silent'],
                            stdout=log, stderr=log, env=env)

    def rpc(method, params=None):
        request = urllib.request.Request(url, json.dumps({
            'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params or []
        }).encode(), {'Content-Type': 'application/json'})
        with urllib.request.urlopen(request, timeout=15) as response:
            value = json.load(response)
        if 'error' in value:
            raise RuntimeError(value['error'])
        return value['result']

    fault = None
    try:
        for _ in range(100):
            try:
                rpc('eth_chainId')
                break
            except OSError:
                time.sleep(0.05)
        assert int(rpc('eth_chainId'), 16) == 6736
        assert 'anvil' in rpc('web3_clientVersion').lower()
        owner = rpc('eth_accounts')[0]

        def receipt(tx):
            for _ in range(100):
                value = rpc('eth_getTransactionReceipt', [tx])
                if value:
                    assert value['status'] == '0x1', value
                    return value
                time.sleep(0.1)
            raise RuntimeError('Fixture deployment receipt timed out')

        def send(data, to=None):
            tx = {'from': owner, 'data': data, 'gas': hex(20_000_000)}
            if to:
                tx['to'] = to
            return receipt(rpc('eth_sendTransaction', [tx]))

        def deploy(source, name, signature=None, *params):
            artifact = json.loads((artifacts/f'{source}.sol'/f'{name}.json').read_text())
            data = artifact['bytecode']['object']
            if signature:
                data += command('cast', 'abi-encode', signature, *params)[2:]
            return send(data)['contractAddress']

        def calldata(signature, *params):
            return command('cast', 'calldata', signature, *params)

        def address(to, signature):
            return command('cast', 'call', '--rpc-url', url, to, signature).split()[0]

        token = deploy('ZoruToken', 'ZoruToken', 'constructor(address)', owner)
        admin = deploy('ZoruAdminAdapter', 'ZoruAdminAdapter', 'constructor(address)', token)
        ledger_impl = deploy('ZoruLedger', 'ZoruLedger')
        engine_impl = deploy('ZoruKeccakEngine', 'ZoruKeccakEngine')
        beacon_impl = deploy('DeepTestHarness', 'DeepTestBeacon')
        launch = int(rpc('eth_getBlockByNumber', ['latest', False])['timestamp'], 16)+20
        deployment = (f'({ledger_impl},{engine_impl},{calldata("initialize()")},'
                      f'{beacon_impl},{calldata("initialize(uint256)", launch)},'
                      f'{launch},{(2**256-1) >> 16})')
        send(calldata('deploySuite((address,address,bytes,address,bytes,uint256,uint256))', deployment), admin)
        ledger = address(admin, 'ledgerProxy()(address)')
        engine = address(admin, 'engineProxy()(address)')
        beacon = address(admin, 'beaconProxy()(address)')
        send(calldata('setIssuer(address)', ledger), token)

        def codehash(addr):
            return command('cast', 'keccak', rpc('eth_getCode', [addr, 'latest']))

        manifest = dict(schema='zoru-mainnet-suite-1', mode='local', chainId=6736,
                        token=token, ledger=ledger, engine=engine, beacon=beacon, admin=admin,
                        owner=owner, launchTime=launch, epochSeconds=60, decimals=8,
                        deploymentBlockNumber='0x0',
                        drandChainHash='04f1e9062b8a81f848fded9c12306733282b2727ecced50032187751166ec8c3',
                        runtimeCodeHashes={k: codehash(v) for k, v in dict(
                            token=token, ledger=ledger, engine=engine, beacon=beacon, admin=admin).items()},
                        approvedImplementationCodeHashes={k: [codehash(v)] for k, v in dict(
                            ledger=ledger_impl, engine=engine_impl, beacon=beacon_impl).items()})
        manifest['manifestSha256'] = hashlib.sha256(json.dumps(
            manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        manifest_path = output/'deployment.json'
        manifest_path.write_text(json.dumps(manifest))
        password = secrets.token_urlsafe(24)
        wallet = output/'disposable.wallet.json'
        created = subprocess.run([str(cli), 'wallet-create', '--wallet', str(wallet), '--password-stdin'],
                                 input=password+'\n', text=True, capture_output=True, env=env, check=True)
        account = json.loads(created.stdout)['data']['address']
        rpc('anvil_setBalance', [account, hex(10**24)])
        rpc('evm_setNextBlockTimestamp', [launch+1])
        rpc('evm_mine')
        fault_url = f'http://127.0.0.1:{port()}'
        control = output/'fault-control.json'
        fault = subprocess.Popen(['python3', str(root/'tests/support/fault_rpc.py'),
                                  '--upstream', url, '--port', fault_url.rsplit(':', 1)[1],
                                  '--control', str(control), '--events', str(output/'fault-events.jsonl'), '--read-through'],
                                 env=env, stdout=log, stderr=log)
        options = dict(manifest=str(manifest_path), read=url, submit=fault_url, wallet=str(wallet),
                       password=password, report=str(output/'recovery.json'), faultControl=str(control), faultRead=fault_url)
        options_path = output/'options.json'
        options_path.write_text(json.dumps(options))
        options_path.chmod(0o600)
        env['ZORU_TEST_OPTIONS'] = str(options_path)
        if args.baseline:
            with (output/'mining.jsonl').open('w') as mining:
                subprocess.run([str(cli), 'mine', '--manifest', str(manifest_path), '--rpc', url,
                                '--state-dir', str(output/'state'), '--account', account,
                                '--dry-run', '--seconds', '20', '--workers', '2', '--cpu-budget', '1'],
                               stdout=mining, stderr=log, env=env, check=True)
        else:
            if args.terminal:
                with (output/'terminal.log').open('w') as terminal_log:
                    subprocess.run(['python3', str(root/'tests/support/terminal.py'), '--options', str(options_path), '--cli', str(cli)],
                                   env=env, stdout=terminal_log, stderr=terminal_log, check=True)
            for name in args.tests:
                wait_for_headroom()
                with (output/f'{name}.log').open('w') as test_log:
                    subprocess.run(['cargo', 'test', '--release', '--locked', '--manifest-path',
                                    str(root/'miner/Cargo.toml'), '--test', name,
                                    '--', '--ignored', '--test-threads=1'], env=env,
                                   stdout=test_log, stderr=test_log, check=True)
            if args.gui_build:
                wait_for_headroom()
                stage = output/'gui'
                (stage/'bin').mkdir(parents=True)
                (stage/'share/zoru-miner').mkdir(parents=True)
                for name in ['zoru-gui-tests', 'zoru-miner-cli']:
                    shutil.copy2(args.gui_build/'bin'/name, stage/'bin'/name)
                shutil.copy2(manifest_path, stage/'share/zoru-miner/deployment.json')
                gui_env = dict(env, ZORU_ISOLATED_UI_OPTIONS=str(options_path),
                               QT_QPA_PLATFORM='offscreen', QT_QUICK_BACKEND='software')
                with (output/'gui.log').open('w') as gui_log:
                    subprocess.run([str(stage/'bin/zoru-gui-tests'),
                                    'isolated_mining_and_claims_use_the_shared_backend'],
                                   env=gui_env, stdout=gui_log, stderr=gui_log, check=True)
        print(json.dumps({'status': 'PASS', 'output': str(output), 'baseline': args.baseline}))
    finally:
        for process in [fault, anvil]:
            if process:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        log.close()


if __name__ == '__main__':
    main()
