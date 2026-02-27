#!/usr/bin/env python3
"""Simple HTTP server built with Bazel for Rush demo."""

from http.server import HTTPServer, BaseHTTPRequestHandler
import json


class HelloHandler(BaseHTTPRequestHandler):
    """Simple handler that returns a hello message."""

    def do_GET(self):
        """Handle GET requests."""
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        
        response = {
            "message": "Hello from Bazel OCI!",
            "build": "Built with Bazel and Rush",
            "path": self.path
        }
        self.wfile.write(json.dumps(response, indent=2).encode())

    def log_message(self, format, *args):
        """Log requests to stdout."""
        print(f"[REQUEST] {args[0]}")


def main():
    """Start the HTTP server."""
    port = 8080
    server = HTTPServer(("0.0.0.0", port), HelloHandler)
    print("=" * 50, flush=True)
    print("Bazel OCI Demo Server is UP and RUNNING!", flush=True)
    print(f"Listening on: http://0.0.0.0:{port}", flush=True)
    print("Built with Bazel and deployed via Rush", flush=True)
    print("=" * 50, flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down server...")
        server.shutdown()


if __name__ == "__main__":
    main()
