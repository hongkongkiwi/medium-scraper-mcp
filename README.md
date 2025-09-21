# Medium Scraper MCP Server

A comprehensive Model Context Protocol (MCP) server for searching Medium articles, converting them to markdown, and bypassing paywalls using proxy services.

## Features

### 🔍 **Article Search**
- Search Medium articles by keywords and tags
- Configurable result limits (1-50 articles)
- Rich article metadata including titles, authors, and snippets

### 📝 **Markdown Conversion**
- Convert Medium articles to clean, readable markdown
- Preserve formatting, code blocks, and images
- Customizable content inclusion (images, code blocks)

### 🚫 **Paywall Bypass**
- Automatic paywall detection
- Multiple proxy services: freedium.cfd, readmedium.com, archive.today
- Configurable proxy preferences with automatic fallback
- Intelligent content validation

### ⚡ **Unified Interface**
- Single tool with multiple operations
- Smart parameter validation
- Comprehensive error handling
- TypeScript implementation with full type safety

## Installation

```bash
# Install dependencies
npm install

# Build the project
npm run build

# Run the server
npm start
```

## Development

```bash
# Install dependencies
npm install

# Run in development mode
npm run dev

# Run tests
npm test

# Lint code
npm run lint

# Type check
npm run type-check
```

## MCP Server Tools

The server provides a single unified tool for all Medium operations:

### `medium_scraper`

A unified tool for searching, converting, and getting Medium article information with paywall bypass capabilities.

**Parameters:**
- `operation` (required): Operation to perform
  - `"search"`: Find articles by query/tag
  - `"convert"`: Convert article to markdown
  - `"info"`: Get article metadata
- `query` (optional): Search query string (required for search)
- `url` (optional): Medium article URL (required for convert/info)
- `tag` (optional): Medium tag to filter by (e.g., "python", "technology") - only for search
- `limit` (optional): Maximum number of results (default: 10, max: 50) - only for search
- `includeImages` (optional): Include image references in markdown (default: true) - only for convert
- `includeCode` (optional): Preserve code blocks (default: true) - only for convert
- `bypassPaywall` (optional): Attempt paywall bypass (default: false) - only for convert
- `preferredProxy` (optional): Preferred proxy service (default: "auto") - only for convert
  - `"freedium"`: Use freedium.cfd
  - `"readmedium"`: Use readmedium.com
  - `"archive"`: Use archive.today
  - `"auto"`: Try all proxies automatically

**Examples:**

#### Search for articles
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "search",
    "query": "machine learning",
    "tag": "python",
    "limit": 5
  }
}
```

#### Convert article to markdown with paywall bypass
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "convert",
    "url": "https://medium.com/@author/article-title-1234567890",
    "includeImages": true,
    "includeCode": true,
    "bypassPaywall": true,
    "preferredProxy": "auto"
  }
}
```

#### Get article information
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "info",
    "url": "https://medium.com/@author/article-title-1234567890"
  }
}
```

## Running the Server

### CLI Mode

```bash
# Build and run
npm run build
npm start

# Development mode
npm run dev

# Verbose logging
npm run dev -- --verbose
```

### MCP Configuration

Add to your MCP client configuration:

```json
{
  "mcpServers": {
    "medium-scraper": {
      "command": "medium-scraper-mcp"
    }
  }
}
```

## Example Usage

### Search for Python articles
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "search",
    "query": "asyncio",
    "tag": "python",
    "limit": 3
  }
}
```

### Convert an article to markdown with paywall bypass
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "convert",
    "url": "https://medium.com/@author/understanding-asyncio-in-python-1234567890",
    "bypassPaywall": true,
    "preferredProxy": "auto"
  }
}
```

### Get article information
```json
{
  "name": "medium_scraper",
  "arguments": {
    "operation": "info",
    "url": "https://medium.com/@author/understanding-asyncio-in-python-1234567890"
  }
}
```

## Dependencies

### Runtime Dependencies
- `@modelcontextprotocol/sdk`: MCP framework
- `axios`: HTTP client
- `cheerio`: HTML parsing
- `turndown`: HTML to Markdown conversion
- `zod`: Schema validation

### Development Dependencies
- `typescript`: TypeScript compiler
- `ts-node`: TypeScript execution
- `jest`: Testing framework
- `eslint`: Code linting
- `@types/*`: TypeScript definitions

## Project Structure

```
src/
├── cli.ts              # Command line interface
├── server.ts           # MCP server implementation
├── medium-scraper.ts   # Medium scraping logic
├── types.ts           # TypeScript type definitions
└── index.ts           # Main exports

dist/                   # Compiled JavaScript files (generated)
├── cli.js
├── server.js
├── medium-scraper.js
├── types.js
└── index.js
```

## API Reference

### MediumScraper Class

#### `searchArticles(params: SearchParams): Promise<Article[]>`

Search for Medium articles.

```typescript
const articles = await mediumScraper.searchArticles({
  query: 'typescript',
  tag: 'programming',
  limit: 10
});
```

#### `convertToMarkdown(params: ConvertParams): Promise<string>`

Convert a Medium article to markdown with optional paywall bypass.

```typescript
const markdown = await mediumScraper.convertToMarkdown({
  url: 'https://medium.com/@author/article-url',
  includeImages: true,
  includeCode: true,
  bypassPaywall: true,
  preferredProxy: 'auto'
});
```

#### `getArticleInfo(url: string): Promise<ArticleInfo>`

Get article metadata.

```typescript
const info = await mediumScraper.getArticleInfo('https://medium.com/@author/article-url');
```

### Paywall Bypass

The server includes intelligent paywall bypass functionality:

**Automatic Detection:**
- Detects paywall indicators in HTML content
- Identifies premium content barriers
- Recognizes subscription prompts

**Proxy Services:**
- **freedium.cfd**: Direct proxy service
- **readmedium.com**: Article-specific proxy
- **archive.today**: Archive-based proxy
- **Auto mode**: Intelligent fallback between services

**Smart Fallback:**
- Tries direct access first
- Falls back to proxy services when paywall detected
- Automatic service switching on failures
- Content validation to ensure quality

## License

MIT License

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run linting and type checking
6. Submit a pull request

## Testing

The project includes Jest tests for all functionality:

```bash
# Run all tests
npm test

# Run with coverage
npm test -- --coverage

# Run tests in watch mode
npm test -- --watch
```

## Error Handling

The server includes comprehensive error handling:

- **Invalid URLs**: Returns user-friendly error messages
- **Network errors**: Graceful handling of connection issues
- **Content parsing**: Handles malformed HTML gracefully
- **Rate limiting**: Respects Medium's rate limits
- **Validation**: Uses Zod schemas for input validation
- **Proxy failures**: Automatic fallback between proxy services
- **Paywall detection**: Graceful degradation when bypass fails
- **Content extraction**: Handles missing or incomplete content

### Error Types

**Network Errors:**
- Connection timeouts (15 seconds for proxy services)
- DNS resolution failures
- HTTP status errors (4xx, 5xx)

**Content Errors:**
- Missing article content
- Malformed HTML structure
- Empty or incomplete responses

**Validation Errors:**
- Invalid URLs (format validation)
- Missing required parameters
- Out-of-range values (e.g., limit > 50)

**Paywall Bypass Errors:**
- All proxy services unavailable
- Content quality validation failures
- Proxy service timeouts

## Performance Considerations

- **Async/Await**: Non-blocking operations for better performance
- **Connection reuse**: Reuses HTTP connections where possible
- **Memory efficient**: Streams responses and processes content incrementally
- **Rate limiting**: Built-in delays to avoid overwhelming Medium's servers