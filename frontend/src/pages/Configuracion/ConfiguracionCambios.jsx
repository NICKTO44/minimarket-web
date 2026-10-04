import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import '../../components/PantallaModulo.css';
import './ConfiguracionDetraccion.css';

const OPCIONES = [3, 7, 15, 30];

/**
 * Configuración → Cambio de prenda (módulo "Cambio de prenda"). El plazo se
 * imprime en cada ticket ("Cambios hasta el ...") y la pantalla de cambios
 * avisa cuando una venta ya lo pasó. 0 = sin plazo.
 */
export default function ConfiguracionCambios({ onMensaje }) {
  const [dias, setDias] = useState(null);
  const [guardado, setGuardado] = useState(null);
  const [guardando, setGuardando] = useState(false);

  useEffect(() => {
    api
      .cambiosConfig()
      .then((c) => {
        setDias(String(c.dias));
        setGuardado(c.dias);
      })
      .catch((e) => onMensaje({ tipo: 'error', texto: e.message }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (dias === null) return null;

  const numero = Number(dias);
  const valido = dias !== '' && Number.isInteger(numero) && numero >= 0 && numero <= 365;

  const guardar = async () => {
    setGuardando(true);
    onMensaje(null);
    try {
      const c = await api.cambiosConfigGuardar(numero);
      setGuardado(c.dias);
      setDias(String(c.dias));
      onMensaje({
        tipo: 'exito',
        texto: c.dias > 0 ? `Plazo de cambio guardado: ${c.dias} días.` : 'Plazo de cambio guardado: sin plazo (no se imprime en el ticket).',
      });
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="cfg-card det-card">
      <div className="det-cabecera">
        <h3 className="cfg-subtitulo-seccion">Cambio de prenda</h3>
      </div>
      <p className="cfg-nota-moneda">
        Días que tiene el cliente para cambiar lo que compró. Se imprime en cada ticket («Cambios hasta el…») y la
        pantalla de cambios avisa si una venta ya pasó el plazo. Escribe 0 si no quieres poner plazo.
      </p>
      <div className="cfg-campo">
        <label>Plazo para cambios (días)</label>
        <input
          inputMode="numeric"
          value={dias}
          onChange={(e) => setDias(e.target.value.replace(/\D/g, '').slice(0, 3))}
          aria-invalid={!valido}
        />
        {!valido && <p className="det-error">Escribe un número de 0 a 365.</p>}
      </div>
      <div className="pm-opciones cam-opciones">
        {OPCIONES.map((d) => (
          <button key={d} type="button" className={numero === d ? 'activo' : ''} onClick={() => setDias(String(d))}>
            {d} días
          </button>
        ))}
      </div>
      <button className="cfg-boton-guardar" onClick={guardar} disabled={!valido || numero === guardado || guardando}>
        {guardando ? 'Guardando...' : 'Guardar plazo'}
      </button>
    </div>
  );
}
