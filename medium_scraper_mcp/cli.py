"""
Command line interface for Medium Scraper MCP Server
"""

import asyncio
import argparse
import sys
from .server import MediumScraperMCPServer

def main():
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Medium Scraper MCP Server"
    )
    parser.add_argument(
        "--stdio",
        action="store_true",
        help="Run in stdio mode (default)"
    )
    parser.add_argument(
        "--port",
        type=int,
        help="Run in HTTP mode on specified port"
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose logging"
    )

    args = parser.parse_args()

    if args.verbose:
        import logging
        logging.basicConfig(level=logging.DEBUG)

    if args.port:
        # HTTP mode (not implemented yet)
        print("HTTP mode not yet implemented", file=sys.stderr)
        sys.exit(1)
    else:
        # stdio mode
        server = MediumScraperMCPServer()
        asyncio.run(server.run())

if __name__ == "__main__":
    main()