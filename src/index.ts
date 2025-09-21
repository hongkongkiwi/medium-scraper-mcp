/**
 * Medium Scraper MCP Server
 *
 * A Model Context Protocol (MCP) server for searching Medium articles
 * and converting them to markdown format.
 */

export { MediumScraper } from './medium-scraper.js';
export { server } from './server.js';
export * from './types.js';

// Version info
export const version = '1.0.0';