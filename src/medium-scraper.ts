import axios from 'axios';
import * as cheerio from 'cheerio';
import TurndownService = require('turndown');
import { Article, ArticleInfo, SearchParams, ConvertParams } from './types';

export class MediumScraper {
  private turndownService: TurndownService;
  private proxyServices = {
    freedium: 'https://freedium.cfd',
    readmedium: 'https://readmedium.com',
    archive: 'https://archive.today'
  };

  constructor() {
    this.turndownService = new TurndownService({
      headingStyle: 'atx',
      bulletListMarker: '-',
      codeBlockStyle: 'fenced'
    });

    // Customize turndown rules for better Medium conversion
    this.turndownService.addRule('strikethrough', {
      filter: ['del', 's', 'strike'],
      replacement: (content: string) => `~~${content}~~`
    });

    this.turndownService.addRule('figure', {
      filter: 'figure',
      replacement: (content: string, node: any) => {
        const $ = cheerio.load(node.outerHTML);
        const img = $('img');
        const figcaption = $('figcaption');

        if (img.length > 0) {
          const src = img.attr('src') || '';
          const alt = img.attr('alt') || '';
          let result = `![${alt}](${src})`;

          if (figcaption.length > 0) {
            result += `\n\n*${figcaption.text().trim()}*`;
          }

          return result;
        }
        return content;
      }
    });
  }

  private extractArticleSlug(url: string): string {
    const match = url.match(/medium\.com\/[^/]+\/([^/?#]+)/);
    return match && match[1] ? match[1] : url;
  }

  private convertToProxyUrl(url: string, proxy: keyof typeof this.proxyServices): string {
    const slug = this.extractArticleSlug(url);

    switch (proxy) {
      case 'freedium':
        return `${this.proxyServices.freedium}/${url}`;
      case 'readmedium':
        return `${this.proxyServices.readmedium}/${slug}`;
      case 'archive':
        return `${this.proxyServices.archive}/${url}`;
      default:
        return url;
    }
  }

  private async isPaywalled(url: string): Promise<boolean> {
    try {
      const response = await axios.get(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36'
        }
      });

      const $ = cheerio.load(response.data);

      // Check for common paywall indicators
      const paywallIndicators = [
        '[data-testid="paywall"]',
        '.paywall',
        '.metered-content',
        '.premium-content',
        'div:contains("premium")',
        'div:contains("member only")',
        'div:contains("subscribe to read")'
      ];

      return paywallIndicators.some(selector =>
        $(selector).length > 0 ||
        response.data.toLowerCase().includes('premium') ||
        response.data.toLowerCase().includes('subscribe to read') ||
        response.data.toLowerCase().includes('member only')
      );
    } catch (error) {
      return false;
    }
  }

  private async tryProxyScraping(url: string, proxyType: keyof typeof this.proxyServices): Promise<string | null> {
    try {
      const proxyUrl = this.convertToProxyUrl(url, proxyType);
      const response = await axios.get(proxyUrl, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36'
        },
        timeout: 15000 // 15 second timeout for proxy services
      });

      if (response.status === 200 && response.data) {
        const $ = cheerio.load(response.data);

        // Check if we got meaningful content
        const content = $('article, .post-content, .content, .article-content').first();
        if (content.length > 0 && content.text().length > 500) {
          return content.html() || response.data;
        }
      }
    } catch (error) {
      console.warn(`Proxy ${proxyType} failed for ${url}:`, error);
    }
    return null;
  }

  async searchArticles(params: SearchParams): Promise<Article[]> {
    const { query, tag, limit = 10 } = params;

    try {
      const searchUrl = 'https://medium.com/search/posts';
      const response = await axios.get(searchUrl, {
        params: { q: tag ? `${query} tag:${tag}` : query },
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36'
        }
      });

      const $ = cheerio.load(response.data);
      const articles: Article[] = [];

      // Medium's search results structure
      $('div[data-test-id="postPreview"]').each((_, element) => {
        if (articles.length >= limit) return;

        const $el = $(element);
        const titleEl = $el.find('h3, h2').first();
        const authorEl = $el.find('a[data-testid="authorName"]').first();
        const urlEl = $el.find('a').first();

        if (titleEl.length > 0 && urlEl.length > 0) {
          const title = titleEl.text().trim();
          let url = urlEl.attr('href') || '';

          // Handle relative URLs
          if (url.startsWith('/')) {
            url = `https://medium.com${url}`;
          }

          const author = authorEl.length > 0 ? authorEl.text().trim() : 'Unknown';
          const snippet = title.length > 100 ? `${title.substring(0, 100)}...` : title;

          articles.push({
            title,
            url,
            author,
            snippet
          });
        }
      });

      return articles;
    } catch (error) {
      console.error('Error searching Medium articles:', error);
      throw new Error(`Failed to search articles: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  }

  async convertToMarkdown(params: ConvertParams): Promise<string> {
    const { url, includeImages = true, includeCode = true, bypassPaywall = false, preferredProxy = 'auto' } = params;

    try {
      let articleContent: string | null = null;
      let isProxyUsed = false;

      // Try direct scraping first
      try {
        const response = await axios.get(url, {
          headers: {
            'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36'
          }
        });

        const $ = cheerio.load(response.data);

        // Check if paywalled and bypass is enabled
        if (bypassPaywall && await this.isPaywalled(url)) {
          console.log('Paywall detected, attempting bypass...');
          isProxyUsed = true;
        } else {
          // Find the main article content
          const content = $('article').first();
          if (content.length > 0) {
            articleContent = content.html();
          }
        }
      } catch (error) {
        if (bypassPaywall) {
          console.log('Direct scraping failed, attempting proxy bypass...');
          isProxyUsed = true;
        } else {
          throw error;
        }
      }

      // Try proxy services if needed
      if (isProxyUsed) {
        const proxyOrder: (keyof typeof this.proxyServices)[] =
          preferredProxy === 'auto' ? ['freedium', 'readmedium', 'archive'] : [preferredProxy];

        for (const proxy of proxyOrder) {
          const content = await this.tryProxyScraping(url, proxy);
          if (content) {
            articleContent = content;
            console.log(`Successfully used ${proxy} proxy`);
            break;
          }
        }

        if (!articleContent) {
          throw new Error('Failed to bypass paywall with all available proxies');
        }
      }

      if (!articleContent) {
        throw new Error('Could not find article content');
      }

      const $ = cheerio.load(articleContent);

      // Extract title (try multiple selectors for different layouts)
      const title = $('h1').first().text().trim() ||
                   $('h2').first().text().trim() ||
                   $('.post-title').first().text().trim() ||
                   'Untitled';

      // Extract author and other metadata (with fallbacks for proxy sites)
      const author = $('a[data-testid="authorName"]').first().text().trim() ||
                     $('.author-name').first().text().trim() ||
                     $('meta[name="author"]').attr('content') ||
                     'Unknown';

      const readingTime = $('span[data-testid="readingTime"]').first().text().trim() ||
                          $('.reading-time').first().text().trim() ||
                          'Unknown';

      const publishDate = $('time').first().attr('datetime') ||
                         $('meta[property="article:published_time"]').attr('content') ||
                         '';

      // Configure turndown based on options
      if (!includeImages) {
        this.turndownService.remove(['img']);
      }

      if (!includeCode) {
        this.turndownService.remove(['pre', 'code']);
      }

      // Convert article content to markdown
      let markdownContent = this.turndownService.turndown(articleContent);

      // Add metadata header
      const result = [
        `# ${title}`,
        '',
        `**Author:** ${author}`,
        `**Reading Time:** ${readingTime}`,
        publishDate ? `**Published:** ${new Date(publishDate).toLocaleDateString()}` : '',
        `**Source:** ${url}`,
        isProxyUsed ? '**Note:** Content retrieved via proxy service (paywall bypass)' : '',
        '',
        '---',
        '',
        markdownContent
      ].filter(Boolean).join('\n');

      return result;
    } catch (error) {
      console.error('Error converting Medium article:', error);
      throw new Error(`Failed to convert article: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  }

  async getArticleInfo(url: string): Promise<ArticleInfo> {
    try {
      const response = await axios.get(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36'
        }
      });

      const $ = cheerio.load(response.data);

      const title = $('h1').first().text().trim() || 'Unknown';
      const author = $('a[data-testid="authorName"]').first().text().trim() || 'Unknown';
      const readingTime = $('span[data-testid="readingTime"]').first().text().trim() || 'Unknown';
      const publishDate = $('time').first().attr('datetime');

      // Estimate word count
      const text = $('article').text() || '';
      const wordCount = text.split(/\s+/).filter(word => word.length > 0).length;

      const result: ArticleInfo = {
        title,
        author,
        readingTime,
        url,
        wordCount
      };

      if (publishDate) {
        result.publishDate = publishDate;
      }

      return result;
    } catch (error) {
      console.error('Error getting article info:', error);
      throw new Error(`Failed to get article info: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  }
}