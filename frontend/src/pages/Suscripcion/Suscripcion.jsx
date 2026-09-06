import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import './Suscripcion.css';

export default function Suscripcion({ estadoSuscripcion, onRecargar, onSuscripcionActivada }) {
  const [codigo, setCodigo] = useState('');
  const [canjeando, setCanjeando] = useState(false);
  const [mensaje, setMensaje] = useState(null);

  useEffect(() => {
    onRecargar?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const canjear = async () => {
    setMensaje(null);
    if (!codigo.trim()) {
      setMensaje({ tipo: 'error', texto: 'Ingresa un código.' });
      return;
    }
    setCanjeando(true);
    try {
      const resultado = await api.canjearCodigo(codigo.trim());
      setMensaje({ tipo: 'exito', texto: resultado.mensaje });
      setCodigo('');
      onSuscripcionActivada?.();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setCanjeando(false);
    }
  };

  if (!estadoSuscripcion) {
    return (
      <div className="susc-layout">
        <p className="susc-cargando">Cargando...</p>
      </div>
    );
  }

  const { estado, fecha_vencimiento, dias_restantes, modo_lectura } = estadoSuscripcion;

  let situacionTexto;
  let situacionClase;

  if (estado === 'SUSPENDIDO') {
    situacionTexto = 'Cuenta suspendida';
    situacionClase = 'susc-badge-bloqueado';
  } else if (modo_lectura) {
    situacionTexto = 'Período vencido';
    situacionClase = 'susc-badge-lectura';
  } else if (fecha_vencimiento == null) {
    situacionTexto = 'Activa (sin vencimiento)';
    situacionClase = 'susc-badge-activo';
  } else {
    situacionTexto = `Activa — vence en ${dias_restantes} día(s)`;
    situacionClase = 'susc-badge-activo';
  }

  return (
    <div className="susc-layout">
      <h1>Suscripción</h1>

      <div className="susc-card">
        <span className={`susc-badge ${situacionClase}`}>{situacionTexto}</span>

        {modo_lectura && (
          <p className="susc-llamado-accion">Activa tu licencia para seguir usando el sistema.</p>
        )}

        {fecha_vencimiento && <p className="susc-fecha">Fecha de vencimiento: {fecha_vencimiento}</p>}

        <div className="susc-separador"></div>

        <h3>Canjear código de activación</h3>
        <p className="susc-nota">
          ¿Tienes un código de Monspeet Dev.? Pégalo aquí para renovar o extender tu suscripción.
        </p>

        {mensaje && <p className={`susc-mensaje susc-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

        <div className="susc-form">
          <input
            value={codigo}
            onChange={(e) => setCodigo(e.target.value.toUpperCase())}
            placeholder="MNSP-XXXX-XXXX-XXXX"
          />
          <button onClick={canjear} disabled={canjeando}>
            {canjeando ? 'Canjeando...' : 'Canjear código'}
          </button>
        </div>
      </div>
    </div>
  );
}