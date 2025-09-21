export interface Article {
  title: string;
  url: string;
  author: string;
  snippet: string;
}

export interface ArticleInfo {
  title: string;
  author: string;
  readingTime: string;
  publishDate?: string;
  url: string;
  wordCount: number;
}

export interface SearchParams {
  query: string;
  tag?: string;
  limit?: number;
}

export interface ConvertParams {
  url: string;
  includeImages?: boolean;
  includeCode?: boolean;
  bypassPaywall?: boolean;
  preferredProxy?: 'freedium' | 'readmedium' | 'archive' | 'auto';
}

export interface UnifiedMediumParams {
  operation: 'search' | 'convert' | 'info';
  query?: string;
  url?: string;
  tag?: string;
  limit?: number;
  includeImages?: boolean;
  includeCode?: boolean;
  bypassPaywall?: boolean;
  preferredProxy?: 'freedium' | 'readmedium' | 'archive' | 'auto';
}