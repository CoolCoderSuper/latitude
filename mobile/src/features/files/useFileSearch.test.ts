import { renderHook, waitFor } from '@testing-library/react-native';

import type { LatitudePublicApi } from '../../api';
import { useFileSearch } from './useFileSearch';

describe('useFileSearch', () => {
  it('debounces content search and exposes result previews', async () => {
    const searchFiles = jest.fn().mockResolvedValue({
      limited: false,
      results: [
        {
          path: 'src/app.ts',
          line: 12,
          column: 5,
          preview: 'const needle = true;',
        },
      ],
    });
    const api = { searchFiles } as unknown as LatitudePublicApi;

    const { result } = await renderHook(() =>
      useFileSearch({
        api,
        projectName: 'demo',
        query: 'needle',
        searchKind: 'grep',
      }),
    );

    expect(searchFiles).not.toHaveBeenCalled();
    await waitFor(() => expect(searchFiles).toHaveBeenCalled());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(searchFiles).toHaveBeenCalledWith(
      'demo',
      'needle',
      'grep',
      expect.any(AbortSignal),
    );
    expect(result.current.results[0]).toEqual(
      expect.objectContaining({
        path: 'src/app.ts',
        preview: 'const needle = true;',
      }),
    );
  });
});
