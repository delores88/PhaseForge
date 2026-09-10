"""Exercise the real local API admission/readiness boundary in an isolated store.
No provider calls or credentials. Requires a compiled backend; does not build it.
"""
import argparse
import hashlib
import hmac
import http.client
import json
import os
import pathlib
import secrets
import socket
import subprocess
import tempfile
import time
from runtime_smoke import backend_process


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    binary = pathlib.Path(args.binary).resolve()
    if not binary.is_file():
        raise SystemExit('A compiled backend is required.')
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        ui_port = listener.getsockname()[1]
    with tempfile.TemporaryDirectory(prefix='phaseforge-api-security-') as directory:
        root = pathlib.Path(directory)
        config = root / 'test.toml'
        config.write_text('bind_address="127.0.0.1"\nport=' + str(port)
                          + '\ndata_directory=' + json.dumps(str(root / 'data'))
                          + '\ngpu_enabled=false\nallowed_frontend_origins='
                          + json.dumps(['http://127.0.0.1:3000', 'http://127.0.0.1:7332',
                                        f'http://127.0.0.1:{ui_port}']) + '\n', encoding='utf-8')
        secret = secrets.token_hex(32)
        environment = {**os.environ, 'PHASEFORGE_PORT': str(port),
                       'PHASEFORGE_BIND': '127.0.0.1',
                       'PHASEFORGE_DATA_DIR': str(root / 'data'),
                       'PHASEFORGE_DESKTOP_SECRET': secret}

        def request(method, path, headers=None):
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
            try:
                connection.request(method, path, headers=headers or {})
                response = connection.getresponse()
                data = response.read()
                return response.status, dict(response.getheaders()), data
            finally:
                connection.close()

        with backend_process([str(binary), '--cpu-only', '--config', str(config)],
                             root / 'backend.log', environment=environment) as process:
            for _ in range(150):
                if process.poll() is not None:
                    raise RuntimeError('Isolated backend exited before readiness.')
                try:
                    if request('GET', '/api/health')[0] == 200:
                        break
                except OSError:
                    pass
                time.sleep(.2)
            else:
                raise RuntimeError('Isolated backend did not become healthy.')
            nonce = secrets.token_hex(32)
            status, headers, body = request('GET', '/api/desktop/ready?nonce=' + nonce)
            proof = json.loads(body)
            expected = hmac.new(bytes.fromhex(secret), nonce.encode(), hashlib.sha256).hexdigest()
            assert status == 200 and hmac.compare_digest(proof['proof'], expected)
            assert proof['nonce'] == nonce and proof['algorithm'] == 'HMAC-SHA256'
            assert headers.get('cache-control') == 'no-store' and secret.encode() not in body
            assert request('GET', '/api/desktop/ready?nonce=bad')[0] == 400
            # Use the production Node verifier and proxy, not a test substitute.
            subprocess.run(
                ['node', str(pathlib.Path(__file__).with_name('api_security_proxy_smoke.cjs'))],
                input=json.dumps({'secret': secret, 'version': proof['version'], 'port': port, 'ui_port': ui_port}),
                text=True, check=True, timeout=45,
            )
            assert request('GET', '/api/health')[0] == 200
            runtime = json.loads(request('GET', '/api/health')[2])['sqlite']
            assert runtime['version'] == '3.53.2' and runtime['version_number'] == 3053002
            assert runtime['linkage'] == 'bundled' and len(runtime['source_id']) > 64
            # The installed desktop forwards this exact Host/Origin pair.
            proxy_headers = {'Host': f'127.0.0.1:{port}', 'Origin': 'http://127.0.0.1:3000'}
            assert request('GET', '/api/providers', proxy_headers)[0] == 200
            assert request('POST', '/api/chat/cancel-all', proxy_headers)[0] == 200
            for path in ['/api/chat/cancel-all', '/api/providers/open_ai/test', '/api/events/ws']:
                method = 'GET' if path.endswith('/ws') else 'POST'
                for origin in ['https://evil.example', 'null', 'http://localhost:3000/']:
                    assert request(method, path, {'Origin': origin})[0] == 403
            assert request('GET', '/api/health', {'Host': f'rebinding.example:{port}'})[0] == 403
            assert request('POST', '/api/chat/cancel-all', {'Sec-Fetch-Site': 'cross-site'})[0] == 403
            assert request('OPTIONS', '/api/projects', {'Origin': 'https://evil.example', 'Access-Control-Request-Method': 'POST'})[0] == 403
            assert request('OPTIONS', '/api/projects', {**proxy_headers, 'Access-Control-Request-Method': 'POST'})[0] == 200
            assert secret.encode() not in request('GET', '/api/health')[2]
            with socket.create_connection(('127.0.0.1', port), timeout=5) as websocket:
                websocket.sendall((f'GET /api/events/ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n'
                                   'Origin: http://127.0.0.1:3000\r\nConnection: Upgrade\r\n'
                                   'Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n'
                                   'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n').encode())
                assert websocket.recv(4096).startswith(b'HTTP/1.1 101')
            assert process.poll() is None
        assert secret not in (root / 'backend.log').read_text(encoding='utf-8', errors='replace')
    print('PASS isolated API: CLI and desktop-proxy access, foreign POST/preflight/WS rejection, rebinding protection, nonce-specific HMAC readiness and clean owned-process exit; no provider calls.')


if __name__ == '__main__':
    main()
