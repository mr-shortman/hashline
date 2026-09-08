try {
  const stored = JSON.parse(
    localStorage.getItem('hashline.preferences') || '{}',
  );
  if (stored.version === 1 && ['light', 'dark'].includes(stored.theme)) {
    document.documentElement.dataset.theme = stored.theme;
  }
} catch {
  /* Damaged preferences must not prevent startup. */
}
