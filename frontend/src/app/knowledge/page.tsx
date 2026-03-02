"use client";

import { useEffect, useState, useCallback } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import type { Collection, Document, SearchResult } from "@/types";
import { apiClient } from "@/lib/api";
import { useToastStore } from "@/stores/toast";
import { formatRelativeTime, formatBytes } from "@/lib/utils";
import {
  BookOpen,
  Search,
  Plus,
  Trash2,
  FileText,
  FolderOpen,
  Upload,
  Database,
  RefreshCw,
  ChevronRight,
  X,
} from "lucide-react";

/* -------------------------------------------------------------------------- */
/*  Collection Card                                                           */
/* -------------------------------------------------------------------------- */

function CollectionCard({
  collection,
  isActive,
  onClick,
  onDelete,
}: {
  collection: Collection;
  isActive: boolean;
  onClick: () => void;
  onDelete: () => void;
}) {
  return (
    <Card
      className={`cursor-pointer transition-all hover:border-border-hover ${
        isActive ? "border-accent bg-background-secondary/50" : ""
      }`}
      onClick={onClick}
    >
      <CardContent className="flex items-center justify-between py-3">
        <div className="flex items-center gap-3">
          <div className="rounded-lg bg-background-secondary p-2 text-accent">
            <FolderOpen className="h-4 w-4" />
          </div>
          <div>
            <p className="text-sm font-medium text-foreground">{collection.name}</p>
            <p className="text-xs text-foreground-muted">
              {collection.document_count} documents
              {collection.description && ` · ${collection.description}`}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 text-foreground-muted hover:text-red-400"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
          <ChevronRight className="h-4 w-4 text-foreground-muted" />
        </div>
      </CardContent>
    </Card>
  );
}

/* -------------------------------------------------------------------------- */
/*  Document Row                                                              */
/* -------------------------------------------------------------------------- */

function DocumentRow({
  document,
  onDelete,
}: {
  document: Document;
  onDelete: () => void;
}) {
  return (
    <div className="flex items-center justify-between border-b border-border/50 px-4 py-3 last:border-0 hover:bg-background-secondary/30 transition-colors">
      <div className="flex items-center gap-3">
        <FileText className="h-4 w-4 text-foreground-muted" />
        <div>
          <p className="text-sm font-medium text-foreground">{document.title}</p>
          <p className="text-xs text-foreground-muted">
            {document.source && `${document.source} · `}
            {document.chunk_count} chunks
            {document.created_at && ` · ${formatRelativeTime(new Date(document.created_at))}`}
          </p>
        </div>
      </div>
      <Button
        size="icon"
        variant="ghost"
        className="h-7 w-7 text-foreground-muted hover:text-red-400"
        onClick={onDelete}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Search Results                                                            */
/* -------------------------------------------------------------------------- */

function SearchResults({
  results,
  query,
}: {
  results: SearchResult[];
  query: string;
}) {
  if (results.length === 0) {
    return (
      <div className="py-8 text-center text-sm text-foreground-muted">
        No results found for &quot;{query}&quot;
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {results.map((r, i) => (
        <Card key={i}>
          <CardContent className="py-3">
            <div className="mb-2 flex items-center justify-between">
              <Badge variant="outline" className="text-xs">
                {r.document_title ?? r.documentName ?? "Unknown source"}
              </Badge>
              <span className="text-xs text-foreground-muted">
                Score: {(r.score * 100).toFixed(1)}%
              </span>
            </div>
            <p className="text-sm leading-relaxed text-foreground-secondary">{r.content || r.chunk}</p>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Upload Document Modal                                                     */
/* -------------------------------------------------------------------------- */

function UploadModal({
  collectionId,
  onSuccess,
  onClose,
}: {
  collectionId: string;
  onSuccess: () => void;
  onClose: () => void;
}) {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [source, setSource] = useState("");
  const [uploading, setUploading] = useState(false);
  const addToast = useToastStore((s) => s.addToast);

  const handleUpload = async () => {
    if (!title.trim() || !content.trim()) return;
    try {
      setUploading(true);
      await apiClient.uploadDocument(collectionId, title.trim(), content.trim(), source.trim() || undefined);
      addToast({ variant: "success", title: "Uploaded", description: `"${title}" added to collection` });
      onSuccess();
      onClose();
    } catch (e) {
      addToast({
        variant: "error",
        title: "Upload failed",
        description: e instanceof Error ? e.message : "Failed to upload document",
      });
    } finally {
      setUploading(false);
    }
  };

  const handleFileRead = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    if (!title.trim()) setTitle(file.name);
    if (!source.trim()) setSource("file-upload");
    const reader = new FileReader();
    reader.onload = (ev) => {
      const text = ev.target?.result;
      if (typeof text === "string") setContent(text);
    };
    reader.readAsText(file);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-lg mx-4 shadow-2xl border-border-hover">
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
          <div>
            <CardTitle className="text-base">Upload Document</CardTitle>
            <CardDescription className="text-xs mt-0.5">
              Add a text document or paste content
            </CardDescription>
          </div>
          <Button size="icon" variant="ghost" onClick={onClose} className="h-8 w-8">
            <X className="h-4 w-4" />
          </Button>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-sm font-medium text-foreground">Title *</label>
            <Input
              placeholder="e.g., Meeting Notes Q1"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-sm font-medium text-foreground">Source</label>
            <Input
              placeholder="e.g., manual, web, file-upload"
              value={source}
              onChange={(e) => setSource(e.target.value)}
            />
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium text-foreground">Content *</label>
              <label className="cursor-pointer text-xs text-accent hover:underline">
                <input type="file" className="hidden" accept=".txt,.md,.csv,.json,.log" onChange={handleFileRead} />
                <Upload className="inline mr-1 h-3 w-3" />
                Import from file
              </label>
            </div>
            <Textarea
              className="min-h-[200px] font-mono text-sm"
              placeholder="Paste or type document content here..."
              value={content}
              onChange={(e) => setContent(e.target.value)}
            />
          </div>

          <div className="flex items-center justify-end gap-2 pt-1">
            <Button variant="ghost" onClick={onClose} disabled={uploading}>
              Cancel
            </Button>
            <Button onClick={handleUpload} disabled={!title.trim() || !content.trim() || uploading}>
              {uploading ? (
                <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Upload className="mr-2 h-4 w-4" />
              )}
              {uploading ? "Uploading..." : "Upload"}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*  Knowledge Base Page                                                       */
/* -------------------------------------------------------------------------- */

export default function KnowledgePage() {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [documents, setDocuments] = useState<Document[]>([]);
  const [activeCollection, setActiveCollection] = useState<string | null>(null);
  const [loadingCollections, setLoadingCollections] = useState(true);
  const [loadingDocuments, setLoadingDocuments] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState("");
  const [showNewCollection, setShowNewCollection] = useState(false);
  const [showUploadModal, setShowUploadModal] = useState(false);

  const addToast = useToastStore((s) => s.addToast);

  const loadCollections = useCallback(async () => {
    try {
      setLoadingCollections(true);
      const data = await apiClient.getCollections();
      setCollections(data);
    } finally {
      setLoadingCollections(false);
    }
  }, []);

  const loadDocuments = useCallback(async (collectionId: string) => {
    try {
      setLoadingDocuments(true);
      const data = await apiClient.getDocuments(collectionId);
      setDocuments(data);
    } finally {
      setLoadingDocuments(false);
    }
  }, []);

  useEffect(() => {
    loadCollections();
  }, [loadCollections]);

  useEffect(() => {
    if (activeCollection) {
      loadDocuments(activeCollection);
    }
  }, [activeCollection, loadDocuments]);

  const handleSearch = async () => {
    if (!searchQuery.trim()) return;
    setSearching(true);
    setSearchResults(null);
    try {
      const results = await apiClient.searchKnowledge(searchQuery, activeCollection ?? undefined);
      setSearchResults(results);
    } finally {
      setSearching(false);
    }
  };

  const handleCreateCollection = async () => {
    if (!newCollectionName.trim()) return;
    try {
      await apiClient.createCollection(newCollectionName.trim());
      addToast({ variant: "success", title: "Created", description: `Collection "${newCollectionName}" created` });
      setNewCollectionName("");
      setShowNewCollection(false);
      await loadCollections();
    } catch (e) {
      addToast({ variant: "error", title: "Failed", description: e instanceof Error ? e.message : "Failed to create collection" });
    }
  };

  const handleDeleteCollection = async (id: string) => {
    try {
      await apiClient.deleteCollection(id);
      if (activeCollection === id) {
        setActiveCollection(null);
        setDocuments([]);
      }
      addToast({ variant: "success", title: "Deleted", description: "Collection removed" });
      await loadCollections();
    } catch (e) {
      addToast({ variant: "error", title: "Failed", description: e instanceof Error ? e.message : "Failed to delete collection" });
    }
  };

  const handleDeleteDocument = async (id: string) => {
    try {
      await apiClient.deleteDocument(id);
      addToast({ variant: "success", title: "Deleted", description: "Document removed" });
      if (activeCollection) {
        await loadDocuments(activeCollection);
      }
    } catch (e) {
      addToast({ variant: "error", title: "Failed", description: e instanceof Error ? e.message : "Failed to delete document" });
    }
  };

  return (
    <div className="mx-auto max-w-6xl space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Knowledge Base</h1>
          <p className="mt-1 text-sm text-foreground-secondary">
            Manage document collections and search your agent&apos;s knowledge.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={loadCollections}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Refresh
          </Button>
        </div>
      </div>

      {/* Search bar */}
      <Card>
        <CardContent className="py-3">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-foreground-muted" />
              <Input
                className="pl-9"
                placeholder="Search across all knowledge..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSearch()}
              />
            </div>
            <Button onClick={handleSearch} disabled={searching || !searchQuery.trim()}>
              {searching ? <RefreshCw className="mr-2 h-4 w-4 animate-spin" /> : <Search className="mr-2 h-4 w-4" />}
              Search
            </Button>
            {searchResults && (
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  setSearchResults(null);
                  setSearchQuery("");
                }}
              >
                <X className="h-4 w-4" />
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Search results */}
      {searchResults && (
        <div>
          <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-foreground-muted">
            Search Results ({searchResults.length})
          </h2>
          <SearchResults results={searchResults} query={searchQuery} />
        </div>
      )}

      {!searchResults && (
        <div className="grid gap-6 lg:grid-cols-3">
          {/* Collections sidebar */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-foreground-muted">
                Collections
              </h2>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setShowNewCollection(!showNewCollection)}
              >
                <Plus className="h-4 w-4" />
              </Button>
            </div>

            {showNewCollection && (
              <div className="flex items-center gap-2">
                <Input
                  placeholder="Collection name"
                  value={newCollectionName}
                  onChange={(e) => setNewCollectionName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleCreateCollection()}
                  autoFocus
                />
                <Button size="sm" onClick={handleCreateCollection}>
                  Add
                </Button>
              </div>
            )}

            {loadingCollections ? (
              <div className="space-y-3">
                {Array.from({ length: 3 }).map((_, i) => (
                  <Skeleton key={i} className="h-16 w-full rounded-lg" />
                ))}
              </div>
            ) : collections.length === 0 ? (
              <div className="py-8 text-center text-sm text-foreground-muted">
                <Database className="mx-auto mb-2 h-8 w-8" />
                No collections yet
              </div>
            ) : (
              <div className="space-y-2">
                {collections.map((c) => (
                  <CollectionCard
                    key={c.id}
                    collection={c}
                    isActive={activeCollection === c.id}
                    onClick={() => setActiveCollection(c.id)}
                    onDelete={() => handleDeleteCollection(c.id)}
                  />
                ))}
              </div>
            )}
          </div>

          {/* Documents list */}
          <div className="lg:col-span-2">
            {activeCollection ? (
              <Card>
                <CardHeader className="flex flex-row items-center justify-between space-y-0">
                  <div>
                    <CardTitle className="text-lg">
                      {collections.find((c) => c.id === activeCollection)?.name ?? "Documents"}
                    </CardTitle>
                    <CardDescription>
                      {documents.length} document{documents.length !== 1 ? "s" : ""}
                    </CardDescription>
                  </div>
                  <Button size="sm" variant="outline" onClick={() => setShowUploadModal(true)}>
                    <Upload className="mr-2 h-4 w-4" />
                    Upload
                  </Button>
                </CardHeader>
                <CardContent className="p-0">
                  {loadingDocuments ? (
                    <div className="space-y-2 p-4">
                      {Array.from({ length: 3 }).map((_, i) => (
                        <Skeleton key={i} className="h-12 w-full" />
                      ))}
                    </div>
                  ) : documents.length === 0 ? (
                    <div className="py-12 text-center text-sm text-foreground-muted">
                      <FileText className="mx-auto mb-2 h-8 w-8" />
                      No documents in this collection
                    </div>
                  ) : (
                    documents.map((doc) => (
                      <DocumentRow
                        key={doc.id}
                        document={doc}
                        onDelete={() => handleDeleteDocument(doc.id)}
                      />
                    ))
                  )}
                </CardContent>
              </Card>
            ) : (
              <Card className="flex h-64 flex-col items-center justify-center text-center">
                <BookOpen className="mb-3 h-10 w-10 text-foreground-muted" />
                <p className="text-sm font-medium text-foreground">
                  Select a collection
                </p>
                <p className="mt-1 text-xs text-foreground-muted">
                  Choose a collection from the left to view its documents.
                </p>
              </Card>
            )}
          </div>
        </div>
      )}

      {/* Upload Modal */}
      {showUploadModal && activeCollection && (
        <UploadModal
          collectionId={activeCollection}
          onSuccess={() => loadDocuments(activeCollection)}
          onClose={() => setShowUploadModal(false)}
        />
      )}
    </div>
  );
}
