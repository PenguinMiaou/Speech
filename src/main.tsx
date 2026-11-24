import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import { ErrorBoundary } from './components/ErrorBoundary'
import { i18nReady } from './i18n'
import './styles/globals.css'

// The language is read before the first frame, so no window shows English first.
void i18nReady
  .catch(() => {})
  .then(() =>
    ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
      <React.StrictMode>
        <ErrorBoundary>
          <App />
        </ErrorBoundary>
      </React.StrictMode>,
    ),
  )
