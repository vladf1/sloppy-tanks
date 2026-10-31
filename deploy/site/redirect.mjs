// The production site's other hostnames. A separate Worker, so the site's own requests
// keep going straight to its static assets instead of running a script to check the host.

export default {
  fetch(request, env) {
    const url = new URL(request.url);
    return Response.redirect(`${env.SITE_ORIGIN}${url.pathname}${url.search}`, 301);
  },
};
