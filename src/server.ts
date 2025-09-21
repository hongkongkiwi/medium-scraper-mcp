import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { z } from 'zod';
import { MediumScraper } from './medium-scraper.js';

const server = new Server(
  {
    name: 'medium-scraper-mcp',
    version: '1.0.0',
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

const mediumScraper = new MediumScraper();

// Tool schema for unified medium scraper
const UnifiedMediumSchema = z.object({
  operation: z.enum(['search', 'convert', 'info'], {
    required_error: 'Operation is required'
  }),
  query: z.string().optional(),
  url: z.string().url('Valid URL is required').optional(),
  tag: z.string().optional(),
  limit: z.number().min(1).max(50).default(10),
  includeImages: z.boolean().default(true),
  includeCode: z.boolean().default(true),
  bypassPaywall: z.boolean().default(false),
  preferredProxy: z.enum(['freedium', 'readmedium', 'archive', 'auto']).default('auto'),
}).refine(data => {
  if (data.operation === 'search') {
    return data.query !== undefined;
  }
  if (data.operation === 'convert' || data.operation === 'info') {
    return data.url !== undefined;
  }
  return true;
}, {
  message: 'Query is required for search operations, URL is required for convert/info operations'
});

// List tools handler
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: 'medium_scraper',
        description: 'Unified Medium scraper tool for searching, converting, and getting article info with paywall bypass',
        inputSchema: {
          type: 'object',
          properties: {
            operation: {
              type: 'string',
              enum: ['search', 'convert', 'info'],
              description: 'Operation to perform: search (find articles), convert (article to markdown), info (get metadata)',
            },
            query: {
              type: 'string',
              description: 'Search query for articles (required for search operation)',
            },
            url: {
              type: 'string',
              description: 'Medium article URL (required for convert and info operations)',
              format: 'uri',
            },
            tag: {
              type: 'string',
              description: 'Medium tag to search (e.g., "python", "technology") - only for search operation',
            },
            limit: {
              type: 'number',
              description: 'Maximum number of results to return - only for search operation',
              default: 10,
              minimum: 1,
              maximum: 50,
            },
            includeImages: {
              type: 'boolean',
              description: 'Whether to include image references in markdown - only for convert operation',
              default: true,
            },
            includeCode: {
              type: 'boolean',
              description: 'Whether to preserve code blocks - only for convert operation',
              default: true,
            },
            bypassPaywall: {
              type: 'boolean',
              description: 'Whether to attempt bypassing Medium paywalls using proxy services - only for convert operation',
              default: false,
            },
            preferredProxy: {
              type: 'string',
              enum: ['freedium', 'readmedium', 'archive', 'auto'],
              description: 'Preferred proxy service for paywall bypass (auto tries all) - only for convert operation',
              default: 'auto',
            },
          },
          required: ['operation'],
        },
      },
    ],
  };
});

// Call tool handler
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    switch (name) {
      case 'medium_scraper': {
        const params = UnifiedMediumSchema.parse(args);

        switch (params.operation) {
          case 'search': {
            const searchParams: any = {
              query: params.query!,
              limit: params.limit
            };
            if (params.tag !== undefined) {
              searchParams.tag = params.tag;
            }
            const articles = await mediumScraper.searchArticles(searchParams);

            return {
              content: [
                {
                  type: 'text',
                  text: JSON.stringify(articles, null, 2),
                },
              ],
            };
          }

          case 'convert': {
            const convertParams: any = {
              url: params.url!,
              includeImages: params.includeImages,
              includeCode: params.includeCode,
              bypassPaywall: params.bypassPaywall,
              preferredProxy: params.preferredProxy
            };
            const markdown = await mediumScraper.convertToMarkdown(convertParams);

            return {
              content: [
                {
                  type: 'text',
                  text: markdown,
                },
              ],
            };
          }

          case 'info': {
            const info = await mediumScraper.getArticleInfo(params.url!);

            return {
              content: [
                {
                  type: 'text',
                  text: JSON.stringify(info, null, 2),
                },
              ],
            };
          }

          default:
            throw new Error(`Unknown operation: ${params.operation}`);
        }
      }

      default:
        throw new Error(`Unknown tool: ${name}`);
    }
  } catch (error) {
    console.error(`Error in tool call ${name}:`, error);

    return {
      content: [
        {
          type: 'text',
          text: `Error: ${error instanceof Error ? error.message : 'Unknown error'}`,
        },
      ],
      isError: true,
    };
  }
});

// Start server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('Medium Scraper MCP server started');
}

main().catch((error) => {
  console.error('Failed to start server:', error);
  process.exit(1);
});

export { server };