-- Forma de pago en los movimientos de la caja (gasto y retiro).
--
-- Pedido de un cliente: si paga a su trabajador por Yape desde la Caja, eso
-- se registraba como efectivo de la caja y no quedaba con qué se pagó.
--
-- Solo ADITIVO: una columna nueva. NULL = efectivo de la caja (como siempre):
-- descuenta el efectivo esperado. Con otra forma de pago (YAPE, PLIN,
-- TRANSFERENCIA, TARJETA, EFECTIVO fuera de caja, OTRO) el movimiento queda
-- solo como registro del turno: no suma a retiros_total ni a gastos_total,
-- así el cuadre del efectivo no cambia.

ALTER TABLE movimientos_caja ADD COLUMN metodo_pago TEXT;
