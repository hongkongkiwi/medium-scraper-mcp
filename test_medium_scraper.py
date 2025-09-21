#!/usr/bin/env python3
"""
Simple test script to verify Medium scraper functionality
"""

import asyncio
import json
import sys
import os
from medium_scraper_mcp.server import MediumScraperMCPServer

async def test_medium_scraper():
    """Test the Medium scraper functionality."""
    server = MediumScraperMCPServer()

    print("Testing Medium Scraper MCP Server...")

    # Test 1: Search for articles
    print("\n1. Testing search_medium_articles...")
    try:
        result = await server._search_medium_articles(
            query="python programming",
            tag="python",
            limit=3
        )
        print("✓ Search successful")
        print(f"Result: {result[0].text[:200]}...")
    except Exception as e:
        print(f"✗ Search failed: {e}")

    # Test 2: Get article info
    print("\n2. Testing get_medium_article_info...")
    try:
        result = await server._get_medium_article_info(
            url="https://medium.com/@pythonhub/understanding-python-async-await-a-beginners-guide-1234567890"
        )
        print("✓ Article info retrieval successful")
        print(f"Result: {result[0].text[:200]}...")
    except Exception as e:
        print(f"✗ Article info retrieval failed: {e}")

    # Test 3: Convert to markdown
    print("\n3. Testing convert_medium_to_markdown...")
    try:
        result = await server._convert_medium_to_markdown(
            url="https://medium.com/@pythonhub/understanding-python-async-await-a-beginners-guide-1234567890"
        )
        print("✓ Markdown conversion successful")
        print(f"Result length: {len(result[0].text)} characters")
        print(f"Preview: {result[0].text[:300]}...")
    except Exception as e:
        print(f"✗ Markdown conversion failed: {e}")

    await server.close()

if __name__ == "__main__":
    asyncio.run(test_medium_scraper())