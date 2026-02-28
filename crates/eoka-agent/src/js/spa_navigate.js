((routerType, path) => {
  const result = { success: false, error: null, newPath: null };

  try {
    switch (routerType) {
      case 'nextjs':
        // Next.js - use next/router
        if (window.next?.router?.push) {
          window.next.router.push(path);
          result.success = true;
          result.newPath = path;
        } else {
          // Fallback for App Router or when router not available
          history.pushState({}, '', path);
          window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
          result.success = true;
          result.newPath = path;
        }
        break;

      case 'vue-router':
        // Vue Router
        const vueApp = document.querySelector('[data-v-app]')?.__vue_app__;
        const router = vueApp?.config?.globalProperties?.$router;
        if (router) {
          router.push(path);
          result.success = true;
          result.newPath = path;
        } else {
          // Vue 2 fallback
          const vue2Router = document.querySelector('#app')?.__vue__?.$router;
          if (vue2Router) {
            vue2Router.push(path);
            result.success = true;
            result.newPath = path;
          } else {
            result.error = 'Vue router not found';
          }
        }
        break;

      case 'react-router':
      case 'angular-router':
      case 'history-api':
      default:
        // Use History API + popstate event (works for most SPAs)
        history.pushState({}, '', path);
        window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
        result.success = true;
        result.newPath = location.pathname;
        break;
    }
  } catch (e) {
    result.error = e.message || String(e);
  }

  return JSON.stringify(result);
})
