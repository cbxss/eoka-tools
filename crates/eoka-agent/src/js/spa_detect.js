(() => {
  const result = {
    router_type: 'unknown',
    path: location.pathname,
    query: Object.fromEntries(new URLSearchParams(location.search)),
    hash: location.hash.slice(1),
    can_navigate: false,
    details: null
  };

  // Check for Remix (superset of React Router v6)
  if (window.__remixContext || window.__remixManifest) {
    result.router_type = 'react-router';
    result.can_navigate = true;
    result.details = 'Remix';
    return JSON.stringify(result);
  }

  // Check for Next.js
  if (window.__NEXT_DATA__ || window.next) {
    result.router_type = 'nextjs';
    result.can_navigate = !!(window.next?.router?.push);
    const isAppRouter = !window.__NEXT_DATA__?.props;
    result.details = isAppRouter ? 'App Router' : 'Pages Router';
    return JSON.stringify(result);
  }

  // Check for Vue 3 with Vue Router
  const vueApp = document.querySelector('[data-v-app]')?.__vue_app__;
  if (vueApp?.config?.globalProperties?.$router) {
    result.router_type = 'vue-router';
    result.can_navigate = true;
    const router = vueApp.config.globalProperties.$router;
    result.details = router.options?.history?.base ? 'Vue Router 4' : 'Vue Router';
    return JSON.stringify(result);
  }

  // Check for Vue 2
  if (window.Vue && document.querySelector('#app')?.__vue__?.$router) {
    result.router_type = 'vue-router';
    result.can_navigate = true;
    result.details = 'Vue 2 Router';
    return JSON.stringify(result);
  }

  // Check for Angular
  if (window.ng || document.querySelector('[ng-version]')) {
    result.router_type = 'angular-router';
    // Angular router access is complex, use History API
    result.can_navigate = true;
    result.details = 'Angular (via History API)';
    return JSON.stringify(result);
  }

  // Check for React Router by looking for common patterns
  // React Router v6 often uses data-reactroot or similar
  const reactRoot = document.querySelector('[data-reactroot], #root, #app');
  if (reactRoot) {
    // Try to detect if React Router is in use by checking for RouterProvider
    // This is heuristic - we look for navigation-related event listeners
    const hasReactRouter = window.__REACT_DEVTOOLS_GLOBAL_HOOK__?.renderers?.size > 0;
    if (hasReactRouter) {
      result.router_type = 'react-router';
      result.can_navigate = true;
      result.details = 'React (via History API)';
      return JSON.stringify(result);
    }
  }

  // Fallback: History API is always available (except file:// protocol)
  if (location.protocol !== 'file:') {
    result.router_type = 'history-api';
    result.can_navigate = true;
    result.details = 'Generic SPA (History API fallback)';
  }

  return JSON.stringify(result);
})()
