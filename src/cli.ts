#!/usr/bin/env node

import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { server } from './server.js';

interface CLIArgs {
  transport?: 'stdio';
  port?: number;
  verbose?: boolean;
}

function parseArgs(): CLIArgs {
  const args = process.argv.slice(2);
  const result: CLIArgs = {};

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];

    switch (arg) {
      case '--stdio':
        result.transport = 'stdio';
        break;
      case '--port':
        if (i + 1 < args.length) {
          const portStr = args[i + 1];
          if (portStr) {
            result.port = parseInt(portStr, 10);
            i++;
          }
        }
        break;
      case '--verbose':
      case '-v':
        result.verbose = true;
        break;
      case '--help':
      case '-h':
        printHelp();
        process.exit(0);
        break;
    }
  }

  return result;
}

function printHelp() {
  console.log(`
Medium Scraper MCP Server

USAGE:
  medium-scraper-mcp [OPTIONS]

OPTIONS:
  --stdio           Run in stdio mode (default)
  --port PORT        Run in HTTP mode on specified port
  --verbose, -v      Enable verbose logging
  --help, -h         Show this help message

EXAMPLES:
  medium-scraper-mcp                    # Run in stdio mode
  medium-scraper-mcp --port 3000       # Run in HTTP mode
  medium-scraper-mcp --verbose          # Enable verbose logging

MCP CONFIGURATION:
Add to your MCP client configuration:

{
  "mcpServers": {
    "medium-scraper": {
      "command": "medium-scraper-mcp"
    }
  }
}
`);
}

async function main() {
  const args = parseArgs();

  if (args.verbose) {
    console.error('Verbose logging enabled');
  }

  try {
    const transport = new StdioServerTransport();
    await server.connect(transport);

    if (args.verbose) {
      console.error('Medium Scraper MCP server started in stdio mode');
    }
  } catch (error) {
    console.error('Failed to start server:', error);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error('Unexpected error:', error);
  process.exit(1);
});