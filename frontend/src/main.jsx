import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './styles/tema.css'
import './index.css'
import App from './App.jsx'
import DialogoConfirmacion from './components/DialogoConfirmacion.jsx'
import { iniciarAppInstalable } from './utils/appInstalable'

iniciarAppInstalable()

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
    <DialogoConfirmacion />
  </StrictMode>,
)