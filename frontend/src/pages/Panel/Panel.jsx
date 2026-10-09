import { useEffect, useState } from 'react';
import { KeyRound, LogOut, ShieldCheck, Store, Ticket } from 'lucide-react';
import { EVENTO_SESION_VENCIDA, guardarSesionPanel, panelApi, sesionPanel } from '../../api/panel';
import PanelNegocios from './PanelNegocios';
import PanelNegocio from './PanelNegocio';
import PanelCodigos from './PanelCodigos';
import './Panel.css';

/**
 * Panel de Monspeet: la pantalla del dueño del sistema, en /panel.
 * Desde aquí se manejan todos los negocios (suscripción y facturación)
 * y los códigos de activación. No usa la sesión de ningún negocio.
 */
export default function Panel() {
  const [sesion, setSesion] = useState(() => sesionPanel());
  const [seccion, setSeccion] = useState('NEGOCIOS');
  const [negocioAbierto, setNegocioAbierto] = useState(null);

  useEffect(() => {
    document.title = 'Panel · Monspeet';
    const alVencer = () => setSesion(null);
    window.addEventListener(EVENTO_SESION_VENCIDA, alVencer);
    return () => window.removeEventListener(EVENTO_SESION_VENCIDA, alVencer);
  }, []);

  const salir = () => {
    guardarSesionPanel(null);
    setSesion(null);
    setNegocioAbierto(null);
  };

  if (!sesion?.token) {
    return (
      <LoginPanel
        onEntrar={(s) => {
          guardarSesionPanel(s);
          setSesion(s);
        }}
      />
    );
  }

  const irA = (s) => {
    setSeccion(s);
    setNegocioAbierto(null);
  };

  return (
    <div className="pnl">
      <header className="pnl-barra">
        <div className="pnl-marca">
          <ShieldCheck size={20} />
          <span>
            Monspeet <em>Panel</em>
          </span>
        </div>
        <nav className="pnl-nav" aria-label="Secciones del panel">
          <button type="button" className={seccion === 'NEGOCIOS' ? 'activo' : ''} onClick={() => irA('NEGOCIOS')}>
            <Store size={16} /> Negocios
          </button>
          <button type="button" className={seccion === 'CODIGOS' ? 'activo' : ''} onClick={() => irA('CODIGOS')}>
            <Ticket size={16} /> Códigos
          </button>
        </nav>
        <div className="pnl-usuario">
          <span>{sesion.usuario}</span>
          <button type="button" onClick={salir} aria-label="Salir del panel" title="Salir">
            <LogOut size={16} />
          </button>
        </div>
      </header>

      <main className="pnl-contenido">
        {seccion === 'NEGOCIOS' && !negocioAbierto && <PanelNegocios onAbrir={setNegocioAbierto} />}
        {seccion === 'NEGOCIOS' && negocioAbierto && (
          <PanelNegocio negocio={negocioAbierto} onVolver={() => setNegocioAbierto(null)} onActualizado={setNegocioAbierto} />
        )}
        {seccion === 'CODIGOS' && <PanelCodigos />}
      </main>
    </div>
  );
}

function LoginPanel({ onEntrar }) {
  const [usuario, setUsuario] = useState('');
  const [clave, setClave] = useState('');
  const [error, setError] = useState('');
  const [entrando, setEntrando] = useState(false);

  const entrar = async (e) => {
    e.preventDefault();
    setError('');
    setEntrando(true);
    try {
      onEntrar(await panelApi.login(usuario, clave));
    } catch (err) {
      setError(err.message);
    } finally {
      setEntrando(false);
    }
  };

  return (
    <div className="pnl-login">
      <form className="pnl-login-caja" onSubmit={entrar}>
        <div className="pnl-login-icono">
          <KeyRound size={22} />
        </div>
        <h1>Panel de Monspeet</h1>
        <p>Solo para el administrador del sistema.</p>
        <label className="pnl-campo">
          <span>Usuario</span>
          <input value={usuario} onChange={(e) => setUsuario(e.target.value)} autoComplete="username" autoFocus />
        </label>
        <label className="pnl-campo">
          <span>Clave</span>
          <input type="password" value={clave} onChange={(e) => setClave(e.target.value)} autoComplete="current-password" />
        </label>
        {error && <p className="pnl-error">{error}</p>}
        <button type="submit" className="pnl-boton pnl-boton-principal" disabled={entrando || !usuario || !clave}>
          {entrando ? 'Entrando...' : 'Entrar'}
        </button>
      </form>
    </div>
  );
}
