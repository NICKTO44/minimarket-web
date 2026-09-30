import { useState } from 'react';

// Ventana con un campo de texto (nombre del cliente, motivo de anulación,
// nota para cocina). alConfirmar puede ser async: si falla, el error se
// muestra aquí mismo y la ventana no se cierra.
export default function DialogoTexto({
  titulo,
  mensaje,
  placeholder,
  valorInicial = '',
  obligatorio = false,
  textoConfirmar = 'Aceptar',
  peligro = false,
  alConfirmar,
  onCerrar,
}) {
  const [valor, setValor] = useState(valorInicial);
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState(null);

  const confirmar = async (e) => {
    e.preventDefault();
    if (obligatorio && !valor.trim()) {
      setError('Este dato es obligatorio.');
      return;
    }
    setGuardando(true);
    setError(null);
    try {
      await alConfirmar(valor.trim());
      onCerrar();
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };

  return (
    <div className="mesas-modal-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="mesas-modal mesas-modal-chico" onSubmit={confirmar}>
        <h2>{titulo}</h2>
        {mensaje && <p className="mesas-modal-mensaje">{mensaje}</p>}
        <input
          className="mesas-input"
          value={valor}
          onChange={(e) => setValor(e.target.value)}
          placeholder={placeholder}
          maxLength={140}
          autoFocus
        />
        {error && <p className="mesas-error">{error}</p>}
        <div className="mesas-modal-acciones">
          <button type="button" className="mesas-boton-secundario" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className={peligro ? 'mesas-boton-peligro' : 'mesas-boton-primario'} disabled={guardando}>
            {guardando ? 'Guardando...' : textoConfirmar}
          </button>
        </div>
      </form>
    </div>
  );
}
