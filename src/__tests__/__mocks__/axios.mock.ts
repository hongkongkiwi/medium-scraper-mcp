// Mock axios for testing
import { AxiosResponse } from 'axios';

const mockAxios = jest.genMockFromModule('axios') as any;

// Set up default mock implementations
mockAxios.get = jest.fn();
mockAxios.create = jest.fn(() => mockAxios);

// Helper function to create mock responses
const createMockResponse = (data: any, status = 200): AxiosResponse => ({
  data,
  status,
  statusText: 'OK',
  headers: {},
  config: { headers: {} as any },
});

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
  malformed: '<invalid>html</content>',
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

// Mock proxy responses
const mockProxyResponses = {
  freedium: `
    <article>
      <h1>Proxy Article Title</h1>
      <div class="author-name">Proxy Author</div>
      <p>Content from proxy service</p>
    </article>
  `,
  readmedium: `
    <article>
      <h1>ReadMedium Article</h1>
      <div>ReadMedium Author</div>
      <p>Content from ReadMedium</p>
    </article>
  `,
  archive: `
    <article>
      <h1>Archive Article</h1>
      <div>Archive Author</div>
      <p>Content from Archive.today</p>
    </article>
  `
};

export { mockAxios, createMockResponse, mockHtmlResponses, mockProxyResponses };