import { z } from 'zod';
import { axiosInstance } from '@/api/axios.ts';
import { type Entry, FORMATS, type Node } from './lib/node.ts';
import type { ConfigFile } from './lib/tabs.ts';

export const configEditorBase = (serverUuid: string) => `/api/client/servers/${serverUuid}/config-editor`;

export const configEditorQueryKey = (serverUuid: string) => ['dev.caloptreyx.configeditor', serverUuid] as const;

const formatSchema = z.enum(FORMATS);

// every payload of this API uses single-word keys, and document keys are arbitrary file keys, so nothing here
// goes through the panel's camelCase/snake_case transformers
const entrySchema: z.ZodType<Entry> = z.lazy(() =>
  z.object({ key: z.string(), value: nodeSchema, comment: z.string().nullable() }),
);

const nodeSchema: z.ZodType<Node> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('string'), value: z.string() }),
    z.object({ kind: z.literal('integer'), value: z.string() }),
    z.object({ kind: z.literal('float'), value: z.string() }),
    z.object({ kind: z.literal('boolean'), value: z.boolean() }),
    z.object({ kind: z.literal('null') }),
    z.object({ kind: z.literal('datetime'), value: z.string() }),
    z.object({ kind: z.literal('array'), items: z.array(nodeSchema) }),
    z.object({ kind: z.literal('object'), entries: z.array(entrySchema) }),
  ]),
);

const configFileSchema: z.ZodType<ConfigFile> = z.object({
  path: z.string(),
  format: formatSchema,
  hash: z.string(),
  content: z.string(),
  document: nodeSchema.nullable(),
  error: z.object({ message: z.string(), line: z.number().nullable(), column: z.number().nullable() }).nullable(),
});

const savedFileSchema = z.intersection(configFileSchema, z.object({ changed: z.boolean() }));
export type SavedFile = z.infer<typeof savedFileSchema>;

const directoryEntrySchema = z.object({
  name: z.string(),
  path: z.string(),
  directory: z.boolean(),
  format: formatSchema.nullable(),
  size: z.number(),
  modified: z.coerce.date(),
});

const directoryListingSchema = z.object({ directory: z.string(), entries: z.array(directoryEntrySchema) });
export type DirectoryListing = z.infer<typeof directoryListingSchema>;

const favoriteSchema = z.object({
  path: z.string(),
  format: formatSchema.nullable(),
  created: z.coerce.date(),
});
export type Favorite = z.infer<typeof favoriteSchema>;

export interface SaveOptions {
  expectedHash: string;
  force?: boolean;
}

export const getDirectory = async (serverUuid: string, directory: string): Promise<DirectoryListing> => {
  const { data } = await axiosInstance.get(`${configEditorBase(serverUuid)}/files`, { params: { directory } });
  return directoryListingSchema.parse(data);
};

export const getConfigFile = async (serverUuid: string, path: string): Promise<ConfigFile> => {
  const { data } = await axiosInstance.get(`${configEditorBase(serverUuid)}/file`, { params: { path } });
  return configFileSchema.parse(data);
};

export const saveDocument = async (
  serverUuid: string,
  path: string,
  document: Node,
  { expectedHash, force = false }: SaveOptions,
  dryRun = false,
): Promise<SavedFile> => {
  const { data } = await axiosInstance.put(`${configEditorBase(serverUuid)}/file`, {
    path,
    expected_hash: expectedHash,
    document,
    dry_run: dryRun,
    force,
  });
  return savedFileSchema.parse(data);
};

export const saveRawContent = async (
  serverUuid: string,
  path: string,
  content: string,
  { expectedHash, force = false }: SaveOptions,
): Promise<SavedFile> => {
  const { data } = await axiosInstance.put(`${configEditorBase(serverUuid)}/file/raw`, {
    path,
    expected_hash: expectedHash,
    content,
    force,
  });
  return savedFileSchema.parse(data);
};

export const getFavorites = async (serverUuid: string): Promise<Favorite[]> => {
  const { data } = await axiosInstance.get(`${configEditorBase(serverUuid)}/favorites`);
  return z.array(favoriteSchema).parse(data.favorites);
};

export const addFavorite = async (serverUuid: string, path: string): Promise<void> => {
  await axiosInstance.post(`${configEditorBase(serverUuid)}/favorites`, { path });
};

export const removeFavorite = async (serverUuid: string, path: string): Promise<void> => {
  await axiosInstance.delete(`${configEditorBase(serverUuid)}/favorites`, { data: { path } });
};
