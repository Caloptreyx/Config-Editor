import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { httpErrorToHuman } from '@/api/axios.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { addFavorite, configEditorQueryKey, getFavorites, removeFavorite } from '../api.ts';

export default function useFavorites(serverUuid: string) {
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const queryKey = [...configEditorQueryKey(serverUuid), 'favorites'];

  const favorites = useQuery({ queryKey, queryFn: () => getFavorites(serverUuid) });
  const paths = new Set(favorites.data?.map((favorite) => favorite.path));

  const toggle = useMutation({
    mutationFn: (path: string) => (paths.has(path) ? removeFavorite(serverUuid, path) : addFavorite(serverUuid, path)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey }),
    onError: (error) => addToast(httpErrorToHuman(error), 'error'),
  });

  return {
    favorites,
    isFavorite: (path: string) => paths.has(path),
    toggleFavorite: toggle.mutate,
    toggling: toggle.isPending,
  };
}
