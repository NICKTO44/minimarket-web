import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, Clock, RotateCcw, Trash2, UserX } from 'lucide-react';
import { registrarDialogo } from '../utils/confirmar';
import './DialogoConfirmacion.css';

// Diálogo de confirmación propio (reemplaza al confirm() del navegador).
// Se monta UNA sola vez en main.jsx; las pantallas lo abren con
// confirmar() de utils/confirmar.js.

const ICONOS = {
  eliminar: Trash2,
  aviso: AlertTriangle,
  tiempo: Clock,
  usuario: UserX,
  reactivar: RotateCcw,
};

export default function DialogoConfirmacion() {
  const [dialogo, setDialogo] = useState(null); // { opciones, resolver }
  const botonCancelarRef = useRef(null);

  useEffect(() => {
    registrarDialogo(
      (opciones) =>
        new Promise((resolver) => {
          setDialogo({ opciones, resolver });
        })
    );
    return () => registrarDialogo(null);
  }, []);

  const cerrar = (resultado) => {
    if (!dialogo) return;
    dialogo.resolver(resultado);
    setDialogo(null);
  };

  useEffect(() => {
    if (!dialogo) return;
    // El foco va a "Cancelar": si alguien presiona Enter sin leer, no se
    // ejecuta la acción peligrosa.
    botonCancelarRef.current?.focus();
    const alPresionarTecla = (e) => {
      if (e.key === 'Escape') {
        dialogo.resolver(false);
        setDialogo(null);
      }
    };
    window.addEventListener('keydown', alPresionarTecla);
    return () => window.removeEventListener('keydown', alPresionarTecla);
  }, [dialogo]);

  if (!dialogo) return null;

  const {
    titulo,
    mensaje,
    detalle,
    textoConfirmar = 'Confirmar',
    textoCancelar = 'Cancelar',
    tipo = 'peligro',
    icono,
  } = dialogo.opciones;
  const Icono = ICONOS[icono] || (tipo === 'peligro' ? AlertTriangle : null);
  const claseIcono = icono === 'aviso' ? 'aviso' : tipo === 'peligro' ? 'peligro' : 'normal';

  return (
    <div className="dlgc-velo" onMouseDown={(e) => e.target === e.currentTarget && cerrar(false)}>
      <div
        className="dlgc-caja"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="dlgc-titulo"
        aria-describedby={mensaje ? 'dlgc-mensaje' : undefined}
      >
        <div className="dlgc-asa" aria-hidden="true" />
        {Icono && (
          <div className={`dlgc-icono dlgc-icono-${claseIcono}`}>
            <Icono size={22} strokeWidth={2} />
          </div>
        )}
        <h3 id="dlgc-titulo" className="dlgc-titulo">
          {titulo}
        </h3>
        {mensaje && (
          <p id="dlgc-mensaje" className="dlgc-mensaje">
            {mensaje}
          </p>
        )}
        {detalle && (
          <div className="dlgc-detalle">
            <span>{detalle.etiqueta}</span>
            <strong>{detalle.valor}</strong>
          </div>
        )}
        <div className="dlgc-acciones">
          <button ref={botonCancelarRef} type="button" className="dlgc-boton" onClick={() => cerrar(false)}>
            {textoCancelar}
          </button>
          <button
            type="button"
            className={`dlgc-boton dlgc-boton-${tipo === 'peligro' ? 'peligro' : 'principal'}`}
            onClick={() => cerrar(true)}
          >
            {textoConfirmar}
          </button>
        </div>
      </div>
    </div>
  );
}
