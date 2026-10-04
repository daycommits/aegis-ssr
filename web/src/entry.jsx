import React from 'react';
import ReactDOMServer from 'react-dom/server.browser';

function App({ title, user }) {
  return (
    <div className="container" style={{ fontFamily: 'system-ui, sans-serif' }}>
      <h1>{title}</h1>
      <p>Welcome back, <strong>{user}</strong>!</p>
      <div className="badge">Rendered server-side with Aegis V8 🛡️</div>
    </div>
  );
}

// Register global render function
globalThis.render = function(propsJson) {
  const props = JSON.parse(propsJson);
  return ReactDOMServer.renderToString(<App {...props} />);
};