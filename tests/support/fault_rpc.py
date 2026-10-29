#!/usr/bin/env python3
"""Drop selected submit responses after an isolated Anvil has accepted them."""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import socket
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--upstream', required=True)
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--control', required=True)
    parser.add_argument('--events', required=True)
    args = parser.parse_args()
    url = urllib.parse.urlsplit(args.upstream)
    if url.scheme != 'http' or url.hostname != '127.0.0.1':
        raise ValueError('The fixture requires a local Anvil endpoint')

    def upstream(body):
        request = urllib.request.Request(args.upstream, json.dumps(body).encode(), {'Content-Type': 'application/json'})
        with urllib.request.urlopen(request, timeout=8) as response:
            data = response.read(1048577)
        if len(data) > 1048576:
            raise ValueError('Response limit exceeded')
        return json.loads(data)

    def rpc(method):
        return upstream({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': []})['result']

    if int(rpc('eth_chainId'), 16) != 6736 or 'anvil' not in rpc('web3_clientVersion').lower():
        raise ValueError('Only isolated Anvil chain 6736 is permitted')
    control = Path(args.control)
    events = Path(args.events)
    control.parent.mkdir(parents=True, exist_ok=True)
    events.parent.mkdir(parents=True, exist_ok=True)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                size = int(self.headers.get('Content-Length', '0'))
                if not 0 < size <= 131072:
                    raise ValueError('Request limit exceeded')
                body = json.loads(self.rfile.read(size))
                method = body.get('method')
                if method not in ('eth_sendRawTransaction', 'eth_chainId', 'eth_gasPrice'):
                    raise ValueError('Only submissions and signing preflight are forwarded')
                response = upstream(body)
                state = json.loads(control.read_text()) if control.exists() else {}
                drop = method == 'eth_sendRawTransaction' and state.get('dropNextSendResponse', 0) > 0
                if drop:
                    state['dropNextSendResponse'] -= 1
                    control.write_text(json.dumps(state))
                with events.open('a') as output:
                    output.write(json.dumps({'dropAfterBackendResponse': drop, 'backendResponse': response})+'\n')
                if drop:
                    self.connection.shutdown(socket.SHUT_RDWR)
                    self.connection.close()
                    return
                data = json.dumps(response).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(data)))
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(data)
            except Exception:
                self.send_response(503)
                self.end_headers()

    HTTPServer(('127.0.0.1', args.port), Handler).serve_forever()


if __name__ == '__main__':
    main()
