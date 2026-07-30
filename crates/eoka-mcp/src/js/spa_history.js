((delta) => {
  const result = { success: false, error: null };
  try {
    history.go(delta);
    result.success = true;
  } catch (e) {
    result.error = e.message || String(e);
  }
  return JSON.stringify(result);
})
