import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import PublishWizard from './PublishWizard.svelte';
import { messages } from '../../lib/i18n';
import { projectFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('PublishWizard', () => {
  it('guides the author through four steps and submits the OpenAPI multipart contract', async () => {
    let submitted: FormData | undefined;
    let idempotencyKey: string | null = null;
    let authorization: string | null = null;

    server.use(
      http.post('http://hub.test/api/v1/projects', async ({ request }) => {
        submitted = await request.formData();
        idempotencyKey = request.headers.get('Idempotency-Key');
        authorization = request.headers.get('Authorization');
        return HttpResponse.json(projectFixture, { status: 201 });
      }),
    );

    render(PublishWizard, { locale: 'ja', m: messages.ja });

    expect(screen.getAllByRole('listitem')).toHaveLength(4);
    const next = screen.getByRole('button', { name: '次へ' });
    expect((next as HTMLButtonElement).disabled).toBe(true);

    const archive = new File(['novnow sample'], 'sample-story.zip', { type: 'application/zip' });
    fireEvent.change(screen.getByLabelText(/ZIP ファイルを選ぶ/), {
      target: { files: [archive] },
    });
    fireEvent.click(next);

    fireEvent.input(screen.getByLabelText(/作品タイトル/), {
      target: { value: '星降る夜の物語' },
    });
    fireEvent.input(screen.getByLabelText('作品の紹介'), {
      target: { value: '小さな町で出会う二人の物語です。' },
    });
    fireEvent.click(screen.getByRole('button', { name: '次へ' }));
    fireEvent.click(screen.getByRole('button', { name: '次へ' }));

    expect(screen.getByRole('heading', { name: '公開前に内容を確認しましょう' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'この内容で公開する' }));

    expect(await screen.findByRole('heading', { name: '作品の公開が完了しました' })).toBeTruthy();
    expect(screen.getByRole('link', { name: projectFixture.publishedUrl }).getAttribute('href')).toBe(
      projectFixture.publishedUrl,
    );
    await waitFor(() => expect(submitted).toBeDefined());
    expect(submitted?.get('title')).toBe('星降る夜の物語');
    expect(submitted?.get('description')).toBe('小さな町で出会う二人の物語です。');
    expect(submitted?.get('license')).toBe('cc-by');
    expect(submitted?.get('visibility')).toBe('public');
    expect(submitted?.get('archive')).toBeTruthy();
    expect(idempotencyKey).toBeTruthy();
    expect(authorization).toBe('Bearer mock-user-token');
  });

  it('rejects files that are not ZIP archives', () => {
    render(PublishWizard, { locale: 'ja', m: messages.ja });
    fireEvent.change(screen.getByLabelText(/ZIP ファイルを選ぶ/), {
      target: { files: [new File(['text'], 'notes.txt', { type: 'text/plain' })] },
    });

    expect(screen.getByRole('alert').textContent).toContain('ZIP ファイル');
    expect((screen.getByRole('button', { name: '次へ' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
