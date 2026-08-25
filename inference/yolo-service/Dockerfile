FROM python:3.11-slim

WORKDIR /app
ENV PYTHONUNBUFFERED=1 \
    YOLO_MODEL=yolov8n.pt \
    YOLO_DEVICE=cpu

COPY inference/yolo-service/requirements.txt ./requirements.txt
RUN pip install --no-cache-dir -r requirements.txt

COPY inference/yolo-service/app ./app

EXPOSE 9000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "9000"]
