"""
Medium Scraper MCP Server

Main MCP server implementation for Medium article search and conversion.
"""

import asyncio
import json
import logging
from typing import Any, Dict, List, Optional
import aiohttp
import httpx
from bs4 import BeautifulSoup
from markdownify import MarkdownConverter
from mcp import ClientSession, StdioServerParameters
from mcp.server import NotificationOptions, Server
from mcp.server.models import InitializationOptions
import mcp.server.stdio
import mcp.types as types

logger = logging.getLogger(__name__)

class MediumScraperMCPServer:
    """MCP server for Medium article scraping and search."""

    def __init__(self):
        self.server = Server("medium-scraper")
        self.session: Optional[aiohttp.ClientSession] = None
        self._setup_handlers()

    def _setup_handlers(self):
        """Set up MCP server handlers."""

        @self.server.list_tools()
        async def list_tools() -> List[types.Tool]:
            """List available tools."""
            return [
                types.Tool(
                    name="search_medium_articles",
                    description="Search for Medium articles by keyword or tag",
                    inputSchema={
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "Search query for articles"
                            },
                            "tag": {
                                "type": "string",
                                "description": "Medium tag to search (e.g., 'python', 'technology')"
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum number of results to return",
                                "default": 10
                            }
                        },
                        "required": ["query"]
                    }
                ),
                types.Tool(
                    name="convert_medium_to_markdown",
                    description="Convert a Medium article URL to markdown",
                    inputSchema={
                        "type": "object",
                        "properties": {
                            "url": {
                                "type": "string",
                                "description": "Medium article URL to convert"
                            },
                            "include_images": {
                                "type": "boolean",
                                "description": "Whether to include image references in markdown",
                                "default": True
                            },
                            "include_code": {
                                "type": "boolean",
                                "description": "Whether to preserve code blocks",
                                "default": True
                            }
                        },
                        "required": ["url"]
                    }
                ),
                types.Tool(
                    name="get_medium_article_info",
                    description="Get metadata about a Medium article without full content",
                    inputSchema={
                        "type": "object",
                        "properties": {
                            "url": {
                                "type": "string",
                                "description": "Medium article URL"
                            }
                        },
                        "required": ["url"]
                    }
                )
            ]

        @self.server.call_tool()
        async def call_tool(
            name: str, arguments: dict | None
        ) -> List[types.TextContent]:
            """Handle tool calls."""
            args = arguments or {}

            try:
                if name == "search_medium_articles":
                    return await self._search_medium_articles(**args)
                elif name == "convert_medium_to_markdown":
                    return await self._convert_medium_to_markdown(**args)
                elif name == "get_medium_article_info":
                    return await self._get_medium_article_info(**args)
                else:
                    raise ValueError(f"Unknown tool: {name}")
            except Exception as e:
                logger.error(f"Error in tool call {name}: {e}")
                return [types.TextContent(
                    type="text",
                    text=f"Error: {str(e)}"
                )]

    async def _get_session(self) -> aiohttp.ClientSession:
        """Get or create HTTP session."""
        if self.session is None:
            self.session = aiohttp.ClientSession(
                timeout=aiohttp.ClientTimeout(total=30)
            )
        return self.session

    async def _search_medium_articles(
        self,
        query: str,
        tag: Optional[str] = None,
        limit: int = 10
    ) -> List[types.TextContent]:
        """Search for Medium articles."""
        try:
            # Use Medium's search API
            search_url = "https://medium.com/search/posts"
            params = {"q": query}
            if tag:
                params["tag"] = tag

            session = await self._get_session()
            async with session.get(search_url, params=params) as response:
                if response.status != 200:
                    raise Exception(f"Search failed: {response.status}")

                # Parse search results from the HTML response
                html = await response.text()
                soup = BeautifulSoup(html, 'html.parser')

                # Extract article information from search results
                articles = []
                article_elements = soup.find_all('div', {'data-test-id': 'postPreview'})

                for element in article_elements[:limit]:
                    try:
                        title_elem = element.find('h3') or element.find('h2')
                        author_elem = element.find('a', {'data-testid': 'authorName'})
                        url_elem = element.find('a')

                        if title_elem and url_elem:
                            title = title_elem.get_text().strip()
                            url = url_elem.get('href', '')
                            if url.startswith('/'):
                                url = f"https://medium.com{url}"

                            author = author_elem.get_text().strip() if author_elem else "Unknown"

                            articles.append({
                                "title": title,
                                "url": url,
                                "author": author,
                                "snippet": title[:100] + "..." if len(title) > 100 else title
                            })
                    except Exception as e:
                        logger.warning(f"Error parsing search result: {e}")
                        continue

                return [types.TextContent(
                    type="text",
                    text=json.dumps(articles, indent=2)
                )]

        except Exception as e:
            logger.error(f"Error searching Medium articles: {e}")
            raise

    async def _convert_medium_to_markdown(
        self,
        url: str,
        include_images: bool = True,
        include_code: bool = True
    ) -> List[types.TextContent]:
        """Convert Medium article to markdown."""
        try:
            # Fetch the article content
            async with httpx.AsyncClient(timeout=30.0) as client:
                response = await client.get(url)
                response.raise_for_status()
                html = response.text

            # Parse HTML
            soup = BeautifulSoup(html, 'html.parser')

            # Find the main article content
            article_content = soup.find('article')
            if not article_content:
                raise Exception("Could not find article content")

            # Extract metadata
            title = soup.find('h1')
            title_text = title.get_text().strip() if title else "Untitled"

            # Convert to markdown
            md = MarkdownConverter(
                heading_style="ATX",
                bullets="-",
                strip=['script', 'style'],
                escape_asterisks=False,
                escape_underscores=False
            )

            markdown_content = md.convert(str(article_content))

            # Add title and metadata
            result = f"# {title_text}\n\n"
            result += f"**Source:** {url}\n\n"
            result += "---\n\n"
            result += markdown_content

            return [types.TextContent(
                type="text",
                text=result
            )]

        except Exception as e:
            logger.error(f"Error converting Medium article: {e}")
            raise

    async def _get_medium_article_info(self, url: str) -> List[types.TextContent]:
        """Get metadata about a Medium article."""
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                response = await client.get(url)
                response.raise_for_status()
                html = response.text

            soup = BeautifulSoup(html, 'html.parser')

            # Extract metadata
            title = soup.find('h1')
            author = soup.find('a', {'data-testid': 'authorName'})
            reading_time = soup.find('span', {'data-testid': 'readingTime'})
            publish_date = soup.find('time')

            info = {
                "title": title.get_text().strip() if title else "Unknown",
                "author": author.get_text().strip() if author else "Unknown",
                "reading_time": reading_time.get_text().strip() if reading_time else "Unknown",
                "publish_date": publish_date.get('datetime') if publish_date else None,
                "url": url,
                "word_count": self._estimate_word_count(html)
            }

            return [types.TextContent(
                type="text",
                text=json.dumps(info, indent=2)
            )]

        except Exception as e:
            logger.error(f"Error getting article info: {e}")
            raise

    def _estimate_word_count(self, html: str) -> int:
        """Estimate word count from HTML content."""
        soup = BeautifulSoup(html, 'html.parser')
        text = soup.get_text()
        words = text.split()
        return len(words)

    async def close(self):
        """Close the server and cleanup resources."""
        if self.session:
            await self.session.close()
            self.session = None

    async def run(self):
        """Run the MCP server."""
        logging.basicConfig(level=logging.INFO)

        async with mcp.server.stdio.stdio_server() as (read_stream, write_stream):
            await self.server.run(
                read_stream,
                write_stream,
                InitializationOptions(
                    server_name="medium-scraper",
                    server_version="0.1.0",
                    capabilities=await self.server.get_capabilities(
                        notification_options=NotificationOptions(),
                        experimental_capabilities={},
                    ),
                ),
            )

async def main():
    """Main entry point for the MCP server."""
    server = MediumScraperMCPServer()
    await server.run()

if __name__ == "__main__":
    asyncio.run(main())