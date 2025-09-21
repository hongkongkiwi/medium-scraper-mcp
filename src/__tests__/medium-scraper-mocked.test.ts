import { MediumScraper } from '../medium-scraper';
import { ArticleInfo } from '../types';

// Mock axios
jest.mock('axios');
const mockedAxios = require('axios');

// Mock HTML responses
const mockHtmlResponses = {
  article: `
    <article>
      <h1>Test Article Title</h1>
      <div data-testid="authorName">Test Author</div>
      <span data-testid="readingTime">5 min read</span>
      <time datetime="2024-01-01T00:00:00Z">January 1, 2024</time>
      <p>This is a test article content with <strong>bold text</strong> and <em>italic text</em>.</p>
      <pre><code>console.log('Hello World');</code></pre>
      <img src="https://example.com/image.jpg" alt="Test image">
      <blockquote>This is a blockquote</blockquote>
    </article>
  `,
  paywalled: `
    <article>
      <h1>Premium Article</h1>
      <div data-testid="paywall">This content is for premium members only</div>
      <p>Some free content here</p>
    </article>
  `,
  searchResults: `
    <div class="streamItem">
      <h3>Search Result 1</h3>
      <a href="https://medium.com/test/article1">Read more</a>
      <p>Snippet 1</p>
    </div>
    <div class="streamItem">
      <h3>Search Result 2</h3>
      <a href="https://medium.com/test/article2">Read more</a>
      <p>Snippet 2</p>
    </div>
  `
};

describe('MediumScraper (Mocked)', () => {
  let scraper: MediumScraper;

  beforeEach(() => {
    scraper = new MediumScraper();
    mockedAxios.get.mockClear();
  });

  describe('searchArticles', () => {
    it('should search articles successfully', async () => {
      // Mock the search API response
      mockedAxios.get.mockResolvedValue({
        data: mockHtmlResponses.searchResults,
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const results = await scraper.searchArticles({
        query: 'typescript',
        limit: 5
      });

      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeLessThanOrEqual(5);
      expect(mockedAxios.get).toHaveBeenCalledTimes(1);
    });

    it('should handle network errors', async () => {
      // Mock network error
      mockedAxios.get.mockRejectedValue(new Error('Network error'));

      await expect(scraper.searchArticles({
        query: 'test',
        limit: 5
      })).rejects.toThrow('Network error');
    });

    it('should handle empty search results', async () => {
      // Mock empty response
      mockedAxios.get.mockResolvedValue({
        data: '<div>No results found</div>',
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const results = await scraper.searchArticles({
        query: 'nonexistent',
        limit: 10
      });

      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBe(0);
    });
  });

  describe('getArticleInfo', () => {
    it('should extract article metadata successfully', async () => {
      // Mock successful article response
      mockedAxios.get.mockResolvedValue({
        data: mockHtmlResponses.article,
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const testUrl = 'https://medium.com/test/article';
      const info: ArticleInfo = await scraper.getArticleInfo(testUrl);

      expect(info).toHaveProperty('title');
      expect(info).toHaveProperty('author');
      expect(info).toHaveProperty('readingTime');
      expect(info).toHaveProperty('url');
      expect(info.url).toBe(testUrl);
      expect(info.title).toBe('Test Article Title');
      expect(info.author).toBe('Test Author');
    });

    it('should handle malformed URLs', async () => {
      // Test with invalid URL
      await expect(scraper.getArticleInfo('invalid-url'))
        .rejects.toThrow();
    });

    it('should handle network failures', async () => {
      // Mock network error
      mockedAxios.get.mockRejectedValue(new Error('Connection failed'));

      await expect(scraper.getArticleInfo('https://medium.com/test/article'))
        .rejects.toThrow('Connection failed');
    });
  });

  describe('convertToMarkdown', () => {
    it('should convert article to markdown successfully', async () => {
      // Mock successful article response
      mockedAxios.get.mockResolvedValue({
        data: mockHtmlResponses.article,
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const markdown = await scraper.convertToMarkdown({
        url: 'https://medium.com/test/article',
        includeImages: true,
        includeCode: true,
        bypassPaywall: false,
        preferredProxy: 'auto'
      });

      expect(typeof markdown).toBe('string');
      expect(markdown).toContain('# Test Article Title');
      expect(markdown).toContain('**Author:** Test Author');
      expect(markdown).toContain('**Reading Time:** 5 min read');
      expect(markdown).toContain('Hello World');
    });

    it('should handle paywall bypass', async () => {
      // Mock paywalled article first
      mockedAxios.get.mockResolvedValueOnce({
        data: mockHtmlResponses.paywalled,
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      // Mock successful proxy response
      mockedAxios.get.mockResolvedValueOnce({
        data: mockHtmlResponses.article,
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const markdown = await scraper.convertToMarkdown({
        url: 'https://medium.com/test/premium',
        includeImages: false,
        includeCode: false,
        bypassPaywall: true,
        preferredProxy: 'freedium'
      });

      expect(typeof markdown).toBe('string');
      expect(mockedAxios.get).toHaveBeenCalledTimes(2);
    });

    it('should handle conversion errors gracefully', async () => {
      // Mock error response
      mockedAxios.get.mockRejectedValue(new Error('Failed to fetch'));

      await expect(scraper.convertToMarkdown({
        url: 'https://medium.com/test/article',
        includeImages: true,
        includeCode: true,
        bypassPaywall: false,
        preferredProxy: 'auto'
      })).rejects.toThrow('Failed to fetch');
    });
  });

  describe('error handling', () => {
    it('should handle malformed HTML gracefully', async () => {
      // Mock malformed HTML response
      mockedAxios.get.mockResolvedValue({
        data: '<invalid>html</content>',
        status: 200,
        statusText: 'OK',
        headers: {},
        config: {},
      });

      const info = await scraper.getArticleInfo('https://medium.com/test/malformed');

      // Should still return basic info without crashing
      expect(info).toHaveProperty('url');
      expect(info.url).toBe('https://medium.com/test/malformed');
    });

    it('should handle timeout scenarios', async () => {
      // Mock timeout error
      mockedAxios.get.mockRejectedValue(new Error('Request timeout'));

      await expect(scraper.getArticleInfo('https://medium.com/test/timeout'))
        .rejects.toThrow('Request timeout');
    });
  });
});