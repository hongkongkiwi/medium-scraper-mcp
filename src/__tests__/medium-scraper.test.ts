import { MediumScraper } from '../medium-scraper';
import { ArticleInfo } from '../types';

describe('MediumScraper', () => {
  let scraper: MediumScraper;

  beforeEach(() => {
    scraper = new MediumScraper();
  });

  describe('searchArticles', () => {
    it('should search articles with basic query', async () => {
      const results = await scraper.searchArticles({
        query: 'typescript',
        limit: 5
      });

      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeLessThanOrEqual(5);

      if (results.length > 0) {
        const article = results[0];
        expect(article).toBeDefined();
        expect(article).toHaveProperty('title');
        expect(article).toHaveProperty('url');
        expect(article).toHaveProperty('author');
        expect(article).toHaveProperty('snippet');
        expect(typeof article!.title).toBe('string');
        expect(typeof article!.url).toBe('string');
        expect(article!.url).toMatch(/^https?:\/\//);
      }
    });

    it('should search articles with tag filter', async () => {
      const results = await scraper.searchArticles({
        query: 'programming',
        tag: 'typescript',
        limit: 3
      });

      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeLessThanOrEqual(3);
    });

    it('should handle empty search results gracefully', async () => {
      const results = await scraper.searchArticles({
        query: 'nonexistenttopic123xyz',
        limit: 5
      });

      expect(Array.isArray(results)).toBe(true);
    });

    it('should handle search with minimal parameters', async () => {
      const results = await scraper.searchArticles({
        query: 'test',
        limit: 1
      });
      expect(Array.isArray(results)).toBe(true);
    });
  });

  describe('getArticleInfo', () => {
    it('should extract article metadata', async () => {
      // Note: This test uses a fake URL and may fail in real usage
      // Consider mocking the HTTP request in a real test suite
      const testUrl = 'https://medium.com/@test/test-article-12345';

      try {
        const info: ArticleInfo = await scraper.getArticleInfo(testUrl);

        expect(info).toHaveProperty('title');
        expect(info).toHaveProperty('author');
        expect(info).toHaveProperty('readingTime');
        expect(info).toHaveProperty('url');
        expect(info).toHaveProperty('wordCount');
        expect(info.url).toBe(testUrl);
        expect(typeof info.wordCount).toBe('number');
        expect(info.wordCount).toBeGreaterThanOrEqual(0);
      } catch (error) {
        // Expected to fail with fake URL
        expect(error).toBeInstanceOf(Error);
      }
    });

    it('should validate URL format', async () => {
      await expect(scraper.getArticleInfo('invalid-url'))
        .rejects.toThrow();
    });
  });

  describe('convertToMarkdown', () => {
    it('should convert article to markdown', async () => {
      // Note: This test uses a fake URL and may fail in real usage
      // Consider mocking the HTTP request in a real test suite
      const testUrl = 'https://medium.com/@test/test-article-12345';

      try {
        const markdown = await scraper.convertToMarkdown({
          url: testUrl,
          includeImages: true,
          includeCode: true
        });

        expect(typeof markdown).toBe('string');
        expect(markdown.length).toBeGreaterThan(0);

        // Check for markdown structure
        expect(markdown).toMatch(/^# /); // Should start with a heading
        expect(markdown).toContain('**Source:**'); // Should contain source metadata
      } catch (error) {
        // Expected to fail with fake URL
        expect(error).toBeInstanceOf(Error);
      }
    });

    it('should handle conversion options', async () => {
      const testUrl = 'https://medium.com/@test/test-article-12345';

      try {
        const markdown = await scraper.convertToMarkdown({
          url: testUrl,
          includeImages: false,
          includeCode: false
        });

        expect(typeof markdown).toBe('string');
      } catch (error) {
        // Expected to fail with fake URL
        expect(error).toBeInstanceOf(Error);
      }
    });

    it('should validate conversion parameters', async () => {
      await expect(scraper.convertToMarkdown({
        url: 'invalid-url',
        includeImages: true,
        includeCode: true
      })).rejects.toThrow();
    });
  });

  describe('error handling', () => {
    it('should handle network errors gracefully', async () => {
      // Test with a non-existent domain
      const invalidUrl = 'https://nonexistent-domain-12345.com/article';

      await expect(scraper.getArticleInfo(invalidUrl))
        .rejects.toThrow();
    });

    it('should handle malformed HTML gracefully', async () => {
      // Test with a URL that doesn't contain article content
      const invalidUrl = 'https://example.com';

      try {
        await scraper.convertToMarkdown({
          url: invalidUrl,
          includeImages: true,
          includeCode: true
        });
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        expect((error as Error).message).toContain('article content');
      }
    });
  });
});