"""
Medium Scraper MCP Server

A Model Context Protocol server for searching Medium articles and converting them to markdown.
"""

from .server import MediumScraperMCPServer

__version__ = "0.1.0"
__all__ = ["MediumScraperMCPServer"]